//! Прокси и дисковый кэш страниц и обложек. Клиент не ходит на чужие сайты.

pub mod cache;
pub mod handlers;

pub use cache::ImageCache;
