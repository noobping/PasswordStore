#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Pages {
    ListPage,
    TextPage,
}

impl Default for Pages {
    fn default() -> Self {
        Pages::ListPage
    }
}
