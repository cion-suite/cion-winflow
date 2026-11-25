use crate::error::AppError;

pub const DEFAULT_PAGE_SIZE: i64 = 50;
pub const MAX_PAGE_SIZE: i64 = 200;

/// Общие правила пагинации для списков: page >= 1, page_size с потолком.
pub fn resolve(page: Option<i64>, page_size: Option<i64>) -> Result<(i64, i64), AppError> {
    let page = page.unwrap_or(1);
    if page < 1 {
        return Err(AppError::bad_request(
            "validation",
            "page must be greater than or equal to 1",
        ));
    }

    let page_size = page_size.unwrap_or(DEFAULT_PAGE_SIZE);
    if page_size < 1 {
        return Err(AppError::bad_request(
            "validation",
            "page_size must be greater than or equal to 1",
        ));
    }

    Ok((page, page_size.min(MAX_PAGE_SIZE)))
}

pub fn offset(page: i64, page_size: i64) -> i64 {
    (page - 1).saturating_mul(page_size)
}
