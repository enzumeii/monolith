use super::*;
use crate::tests::product;

#[test]
fn search_includes_description_type_variant_and_sku_case_insensitively() {
    let products = [product()];
    for query in [" SHIRT ", "cotton", "clothing", "BLUE", "shirt-bl-l"] {
        let filters = Filters {
            search: query.into(),
            ..Filters::default()
        };
        assert_eq!(filters.matching(&products).len(), 1, "query {query}");
    }
    let filters = Filters {
        search: "missing".into(),
        ..Filters::default()
    };
    assert_eq!(filters.matching(&products), [] as [&PublicProduct; 0]);
}

#[test]
fn type_filter_combines_with_search_and_pagination_clamps_empty_or_shrunk_results() {
    let products = [product()];
    let mut filters = Filters {
        product_type: "Prints".into(),
        page: 9,
        ..Filters::default()
    };
    assert_eq!(filters.matching(&products), [] as [&PublicProduct; 0]);
    assert_eq!(filters.visible_page(0), 1);
    assert_eq!(filters.visible_page(25), 2);
    filters.reset();
    assert_eq!(filters.page, 1);
    assert_eq!(
        product_types(&products).into_iter().collect::<Vec<_>>(),
        vec!["Clothing"]
    );
}
