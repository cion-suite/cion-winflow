-- Диапазонные связки глав: одна строка описывает правило
-- «с этой главы источника и выше». Точные пары остаются в chapter_aliases
-- как ручные (source_entry_id IS NULL) или производные (origin = 'range').

CREATE TABLE chapter_alias_ranges (
    id                      INTEGER PRIMARY KEY,
    title_id                INTEGER NOT NULL REFERENCES titles (id) ON DELETE CASCADE,
    source_entry_id         INTEGER NOT NULL REFERENCES source_entries (id) ON DELETE CASCADE,
    anchor_chapter_id       INTEGER NOT NULL REFERENCES chapters (id) ON DELETE CASCADE,
    anchor_chapter_key      TEXT    NOT NULL,
    anchor_volume           INTEGER,
    anchor_number_text      TEXT,
    anchor_number_milli     INTEGER,
    anchor_ordinal_site     INTEGER NOT NULL DEFAULT 0,
    canonical_key           TEXT    NOT NULL,
    canonical_chapter_id    INTEGER REFERENCES chapters (id) ON DELETE SET NULL,
    canonical_volume        INTEGER,
    canonical_number_text   TEXT,
    canonical_number_milli  INTEGER,
    chapter_kind            TEXT    NOT NULL,
    mapping_mode            TEXT    NOT NULL DEFAULT 'volume_number'
                            CHECK (mapping_mode IN ('volume_number', 'number_only')),
    status                  TEXT    NOT NULL DEFAULT 'active'
                            CHECK (status IN ('active', 'needs_review', 'broken')),
    created_at              INTEGER NOT NULL
);

CREATE UNIQUE INDEX chapter_alias_ranges_anchor
    ON chapter_alias_ranges (title_id, source_entry_id, chapter_kind, anchor_chapter_id);
CREATE INDEX chapter_alias_ranges_title ON chapter_alias_ranges (title_id);

-- Производные пары привязаны к source_entry: один и тот же chapter_key
-- у двух источников не должен перетирать друг друга. Ручные пары остаются
-- глобальными (source_entry_id IS NULL) и работают как раньше.
ALTER TABLE chapter_aliases ADD COLUMN source_entry_id INTEGER
    REFERENCES source_entries (id) ON DELETE CASCADE;
ALTER TABLE chapter_aliases ADD COLUMN origin TEXT NOT NULL DEFAULT 'manual'
    CHECK (origin IN ('manual', 'legacy', 'range'));

DROP INDEX chapter_aliases_alias;
CREATE UNIQUE INDEX chapter_aliases_global_alias
    ON chapter_aliases (title_id, alias_key) WHERE source_entry_id IS NULL;
CREATE UNIQUE INDEX chapter_aliases_entry_alias
    ON chapter_aliases (title_id, source_entry_id, alias_key)
    WHERE source_entry_id IS NOT NULL;
