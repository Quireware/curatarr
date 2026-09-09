use curatarr_core::types::Pagination;
use serde::Deserialize;

pub const MAX_PER_PAGE: u32 = 500;

/// `?page=1&per_page=20` — shared by every list endpoint.
#[derive(Debug, Clone, Default, Deserialize)]
pub struct PageQuery {
    pub page: Option<u32>,
    pub per_page: Option<u32>,
}

impl PageQuery {
    pub fn pagination(&self) -> Pagination {
        let defaults = Pagination::default();
        Pagination {
            page: self.page.unwrap_or(defaults.page).max(1),
            per_page: self
                .per_page
                .unwrap_or(defaults.per_page)
                .clamp(1, MAX_PER_PAGE),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use proptest::prelude::*;

    proptest! {
        #[test]
        fn pagination_is_always_in_range(page in any::<Option<u32>>(), per_page in any::<Option<u32>>()) {
            let p = PageQuery { page, per_page }.pagination();
            prop_assert!(p.page >= 1);
            prop_assert!((1..=MAX_PER_PAGE).contains(&p.per_page));
        }
    }

    #[test]
    fn defaults_apply_when_absent() {
        let p = PageQuery::default().pagination();
        assert_eq!(p, Pagination::default());
    }
}
