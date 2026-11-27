#![cfg(debug_assertions)]

mod common;

use axum::http::{Method, StatusCode};
use serde_json::{Value, json};

use cion_reader::auth::session::now_unix;
use common::{TestApp, body_json, get, json_request, with_cookie};

async fn configure(app: &TestApp, admin: &str, code: &str, base_url: &str) {
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/sources/{code}"),
                &json!({ "base_url": base_url, "enabled": true }),
            ),
            admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "configure {code}");
}

async fn seed_entry(
    app: &TestApp,
    title_id: i64,
    code: &str,
    volume: i64,
    numbers: &[i64],
) -> (i64, Vec<i64>) {
    let source_id = sqlx::query_scalar::<_, i64>("SELECT id FROM sources WHERE code = ?")
        .bind(code)
        .fetch_one(&app.pool)
        .await
        .expect("source id");
    let now = now_unix();
    let chapter_count = i64::try_from(numbers.len()).expect("chapter count");

    let entry_id = sqlx::query_scalar::<_, i64>(
        "INSERT INTO source_entries \
         (title_id, source_id, external_id, is_primary, chapter_count, available_count, \
          stats_updated_at, last_synced_at) \
         VALUES (?, ?, ?, 0, ?, ?, ?, ?) RETURNING id",
    )
    .bind(title_id)
    .bind(source_id)
    .bind(format!("{code}-range"))
    .bind(chapter_count)
    .bind(chapter_count)
    .bind(now)
    .bind(now)
    .fetch_one(&app.pool)
    .await
    .expect("source entry");

    let mut ids = Vec::with_capacity(numbers.len());
    for (ordinal, number) in numbers.iter().enumerate() {
        let id = sqlx::query_scalar::<_, i64>(
            "INSERT INTO chapters \
             (source_entry_id, external_id, chapter_key, number_text, number_milli, \
              volume, ordinal_site, chapter_kind, content_kind, availability, last_seen_at) \
             VALUES (?, ?, ?, ?, ?, ?, ?, 'normal', 'images', 'available', ?) RETURNING id",
        )
        .bind(entry_id)
        .bind(format!("{code}-{number}"))
        .bind(format!("v{volume}:c{number}"))
        .bind(number.to_string())
        .bind(number * 1000)
        .bind(volume)
        .bind(i64::try_from(ordinal).expect("ordinal"))
        .bind(now)
        .fetch_one(&app.pool)
        .await
        .expect("chapter");
        ids.push(id);
    }

    (entry_id, ids)
}

async fn insert_chapter(app: &TestApp, entry_id: i64, code: &str, volume: i64, number: i64) -> i64 {
    let now = now_unix();
    sqlx::query_scalar::<_, i64>(
        "INSERT INTO chapters \
         (source_entry_id, external_id, chapter_key, number_text, number_milli, \
          volume, ordinal_site, chapter_kind, content_kind, availability, last_seen_at) \
         VALUES (?, ?, ?, ?, ?, ?, 99, 'normal', 'images', 'available', ?) RETURNING id",
    )
    .bind(entry_id)
    .bind(format!("{code}-{number}"))
    .bind(format!("v{volume}:c{number}"))
    .bind(number.to_string())
    .bind(number * 1000)
    .bind(volume)
    .bind(now)
    .fetch_one(&app.pool)
    .await
    .expect("chapter")
}

async fn get_json(app: &TestApp, uri: &str, admin: &str) -> Value {
    let response = app.request(with_cookie(get(uri), admin)).await;
    assert_eq!(response.status(), StatusCode::OK, "{uri}");
    body_json(response).await
}

fn chapter_group<'a>(title: &'a Value, key: &str) -> &'a Value {
    title["chapters"]
        .as_array()
        .expect("chapters")
        .iter()
        .find(|chapter| chapter["chapter_key"].as_str() == Some(key))
        .unwrap_or_else(|| panic!("chapter group {key} not found"))
}

fn variant_keys(group: &Value) -> Vec<String> {
    group["variants"]
        .as_array()
        .expect("variants")
        .iter()
        .filter_map(|variant| variant["chapter_key"].as_str().map(str::to_owned))
        .collect()
}

#[tokio::test]
async fn single_manual_alias_is_returned_separately() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure(&app, &admin, "remanga", "https://api.remanga.org").await;
    configure(&app, &admin, "rumix", "https://rumix.me").await;

    let title_id = app.create_title(&admin, "Single alias", "manga").await;
    seed_entry(&app, title_id, "remanga", 3, &[151]).await;
    seed_entry(&app, title_id, "rumix", 2, &[151]).await;

    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases"),
                &json!({ "alias_key": "v2:c151", "canonical_key": "v3:c151" }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "create manual alias");

    let ranges = get_json(
        &app,
        &format!("/api/titles/{title_id}/alias-ranges"),
        &admin,
    )
    .await;
    assert!(ranges["ranges"].as_array().expect("ranges").is_empty());
    let manual = ranges["manual_aliases"].as_array().expect("manual aliases");
    assert_eq!(manual.len(), 1);
    assert_eq!(manual[0]["alias_key"].as_str(), Some("v2:c151"));
}

#[tokio::test]
async fn legacy_aliases_collapse_into_one_range() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure(&app, &admin, "remanga", "https://api.remanga.org").await;
    configure(&app, &admin, "rumix", "https://rumix.me").await;

    let title_id = app.create_title(&admin, "Legacy range", "manga").await;
    seed_entry(&app, title_id, "remanga", 3, &[151, 152, 153, 154]).await;
    seed_entry(&app, title_id, "rumix", 2, &[151, 152, 153, 154]).await;

    let aliases: Vec<Value> = (151..=154)
        .map(|number| {
            json!({
                "alias_key": format!("v2:c{number}"),
                "canonical_key": format!("v3:c{number}"),
            })
        })
        .collect();
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/chapter-aliases/bulk"),
                &json!({ "aliases": aliases }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "create legacy aliases");

    let ranges = get_json(
        &app,
        &format!("/api/titles/{title_id}/alias-ranges"),
        &admin,
    )
    .await;
    let items = ranges["ranges"].as_array().expect("ranges");
    assert_eq!(items.len(), 1, "four pairs collapse into one range");
    assert_eq!(items[0]["anchor"]["chapter_key"].as_str(), Some("v2:c151"));
    assert_eq!(
        items[0]["canonical"]["chapter_key"].as_str(),
        Some("v3:c151")
    );
    assert_eq!(items[0]["coverage"]["linked"].as_i64(), Some(4));

    let manual = get_json(
        &app,
        &format!("/api/titles/{title_id}/chapter-aliases"),
        &admin,
    )
    .await;
    assert!(
        manual["aliases"].as_array().expect("aliases").is_empty(),
        "converted aliases are removed from the legacy table"
    );

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let group = chapter_group(&title, "v3:c154");
    assert_eq!(
        variant_keys(group).len(),
        2,
        "legacy alias still groups the variant"
    );
}

#[tokio::test]
async fn range_links_new_chapters_overrides_and_remaps_progress() {
    let app = TestApp::new().await;
    app.create_admin().await;
    let admin = app.login_admin().await;
    configure(&app, &admin, "remanga", "https://api.remanga.org").await;
    configure(&app, &admin, "rumix", "https://rumix.me").await;

    let title_id = app.create_title(&admin, "Range title", "manga").await;
    let (canonical_entry, canonical_ids) =
        seed_entry(&app, title_id, "remanga", 3, &[151, 152, 153, 154]).await;
    let (linked_entry, linked_ids) =
        seed_entry(&app, title_id, "rumix", 2, &[151, 152, 153, 154]).await;

    // Прогресс до связки: читатель открыл главу под ключом источника.
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/progress/{title_id}"),
                &json!({ "chapter_key": "v2:c151", "page": 0 }),
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "save progress");

    let first = json!({
        "source_entry_id": linked_entry,
        "anchor_chapter_id": linked_ids[0],
        "canonical_chapter_id": canonical_ids[0],
    });
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/alias-ranges"),
                &first,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "create range");
    let range = body_json(response).await;
    let first_range_id = range["id"].as_i64().expect("range id");
    assert_eq!(range["coverage"]["linked"], 4, "all four chapters linked");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let group = chapter_group(&title, "v3:c151");
    let keys = variant_keys(group);
    assert_eq!(keys.len(), 2, "anchor group has both sources: {keys:?}");
    assert!(keys.contains(&"v2:c151".to_owned()));
    assert!(keys.contains(&"v3:c151".to_owned()));
    assert_eq!(group["number_text"].as_str(), Some("151"));

    let progress = get_json(&app, &format!("/api/progress/{title_id}"), &admin).await;
    assert_eq!(
        progress["progress"]["chapter_key"].as_str(),
        Some("v3:c151"),
        "progress moved to canonical key"
    );

    // Новая глава появилась в обоих источниках; любое обновление связки
    // пересобирает производные пары.
    insert_chapter(&app, linked_entry, "rumix", 2, 155).await;
    insert_chapter(&app, canonical_entry, "remanga", 3, 155).await;
    let response = app
        .request(with_cookie(
            json_request(
                Method::PUT,
                &format!("/api/titles/{title_id}/alias-ranges/{first_range_id}"),
                &first,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "touch range");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let new_group = chapter_group(&title, "v3:c155");
    assert_eq!(
        variant_keys(new_group).len(),
        2,
        "new chapter joined the range automatically"
    );

    // Вторая связка перекрывает первую: v2:c153 → v3:c154, v2:c154 → v3:c155.
    let second = json!({
        "source_entry_id": linked_entry,
        "anchor_chapter_id": linked_ids[2],
        "canonical_chapter_id": canonical_ids[3],
    });
    let response = app
        .request(with_cookie(
            json_request(
                Method::POST,
                &format!("/api/titles/{title_id}/alias-ranges"),
                &second,
            ),
            &admin,
        ))
        .await;
    assert_eq!(response.status(), StatusCode::OK, "create second range");
    let second_range = body_json(response).await;
    let second_range_id = second_range["id"].as_i64().expect("second range id");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let overridden = chapter_group(&title, "v3:c154");
    assert!(
        variant_keys(overridden).contains(&"v2:c153".to_owned()),
        "second range overrides the first above its anchor"
    );
    let shifted = chapter_group(&title, "v3:c155");
    assert!(
        variant_keys(shifted).contains(&"v2:c154".to_owned()),
        "second range shifts the following chapter"
    );

    let ranges = get_json(
        &app,
        &format!("/api/titles/{title_id}/alias-ranges"),
        &admin,
    )
    .await;
    let first_dto = ranges["ranges"]
        .as_array()
        .expect("ranges")
        .iter()
        .find(|range| range["id"].as_i64() == Some(first_range_id))
        .expect("first range dto");
    assert_eq!(
        first_dto["next_range_id"].as_i64(),
        Some(second_range_id),
        "first range is bounded by the second"
    );

    // Удаление второй связки возвращает главу под первую.
    let delete_request = axum::http::Request::builder()
        .method(Method::DELETE)
        .uri(format!(
            "/api/titles/{title_id}/alias-ranges/{second_range_id}"
        ))
        .header(axum::http::header::ORIGIN, common::ORIGIN)
        .header("sec-fetch-site", "same-origin")
        .body(axum::body::Body::empty())
        .expect("delete request");
    let response = app.request(with_cookie(delete_request, &admin)).await;
    assert_eq!(response.status(), StatusCode::NO_CONTENT, "delete range");

    let title = get_json(&app, &format!("/api/titles/{title_id}"), &admin).await;
    let back = chapter_group(&title, "v3:c153");
    assert!(
        variant_keys(back).contains(&"v2:c153".to_owned()),
        "first range applies again after the second is deleted"
    );
}
