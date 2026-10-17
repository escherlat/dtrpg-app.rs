//! Data transformation helpers for library presentation.

use crate::services::LibraryItem;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum LibraryViewMode {
    FlatList,
    TreeByPublisher,
    TreeByProductType,
    GridByPublisher,
    GridByProductType,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum FilterScope {
    ChildOnly,
    RootAndChild,
    RootOnly,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum MatchPresentation {
    /// Hide non-matching items and show only matching results.
    HideNonMatching,
    /// Keep all items and visually highlight matching results.
    HighlightMatching,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SortMethod {
    AtoZ,
    ZtoA,
    MostRecentlyAdded,
    MostRecentlyUpdated,
}

#[derive(Clone, Debug)]
pub struct TreeNode {
    pub root_label: String,
    pub children: Vec<LibraryItem>,
}

pub fn next_sort(current: SortMethod) -> SortMethod {
    match current {
        SortMethod::AtoZ => SortMethod::ZtoA,
        SortMethod::ZtoA => SortMethod::MostRecentlyAdded,
        SortMethod::MostRecentlyAdded => SortMethod::MostRecentlyUpdated,
        SortMethod::MostRecentlyUpdated => SortMethod::AtoZ,
    }
}

pub fn sort_label(sort: SortMethod) -> &'static str {
    match sort {
        SortMethod::AtoZ => "A-Z",
        SortMethod::ZtoA => "Z-A",
        SortMethod::MostRecentlyAdded => "Most recently added",
        SortMethod::MostRecentlyUpdated => "Most recently updated",
    }
}

pub fn mode_label(mode: LibraryViewMode) -> &'static str {
    match mode {
        LibraryViewMode::FlatList => "Flat list",
        LibraryViewMode::TreeByPublisher => "Tree by publisher",
        LibraryViewMode::TreeByProductType => "Tree by product type",
        LibraryViewMode::GridByPublisher => "Grid by publisher",
        LibraryViewMode::GridByProductType => "Grid by product type",
    }
}

/// Returns whether the view mode renders library content as grid cells.
pub fn mode_is_grid(mode: LibraryViewMode) -> bool {
    matches!(
        mode,
        LibraryViewMode::GridByPublisher | LibraryViewMode::GridByProductType
    )
}

pub fn filter_presets() -> [&'static str; 4] {
    ["", "atlas", "sandbox", "tabletop"]
}

pub fn sorted_flat_items(items: &[LibraryItem], sort: SortMethod) -> Vec<LibraryItem> {
    let mut result = items.to_vec();
    sort_items(&mut result, sort);
    result
}

pub fn grouped_items(
    items: &[LibraryItem],
    mode: LibraryViewMode,
    outer_sort: SortMethod,
    inner_sort: SortMethod,
) -> Vec<TreeNode> {
    let mut groups = std::collections::BTreeMap::<String, Vec<LibraryItem>>::new();

    for item in items.iter().cloned() {
        let key = match mode {
            LibraryViewMode::TreeByPublisher => item.publisher.clone(),
            LibraryViewMode::GridByPublisher => item.publisher.clone(),
            LibraryViewMode::TreeByProductType => item.product_type.clone(),
            LibraryViewMode::GridByProductType => item.product_type.clone(),
            LibraryViewMode::FlatList => item.publisher.clone(),
        };
        groups.entry(key).or_default().push(item);
    }

    let mut nodes = Vec::new();

    for (root_label, mut children) in groups {
        sort_items(&mut children, inner_sort);
        nodes.push(TreeNode {
            root_label,
            children,
        });
    }

    nodes.sort_by(|a, b| compare_roots(&a.root_label, &b.root_label, outer_sort));
    nodes
}

pub fn item_matches(item: &LibraryItem, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    let query = query.to_lowercase();
    item.title.to_lowercase().contains(&query)
        || item.publisher.to_lowercase().contains(&query)
        || item.product_type.to_lowercase().contains(&query)
}

pub fn root_matches(root_label: &str, query: &str) -> bool {
    if query.is_empty() {
        return true;
    }

    root_label.to_lowercase().contains(&query.to_lowercase())
}

fn sort_items(items: &mut [LibraryItem], sort: SortMethod) {
    items.sort_by(|a, b| compare_items(a, b, sort));
}

fn compare_items(a: &LibraryItem, b: &LibraryItem, sort: SortMethod) -> std::cmp::Ordering {
    match sort {
        SortMethod::AtoZ => a.title.cmp(&b.title),
        SortMethod::ZtoA => b.title.cmp(&a.title),
        SortMethod::MostRecentlyAdded => b.added_order.cmp(&a.added_order),
        SortMethod::MostRecentlyUpdated => b.updated_order.cmp(&a.updated_order),
    }
}

fn compare_roots(a: &str, b: &str, sort: SortMethod) -> std::cmp::Ordering {
    match sort {
        SortMethod::AtoZ => a.cmp(b),
        SortMethod::ZtoA => b.cmp(a),
        SortMethod::MostRecentlyAdded => b.cmp(a),
        SortMethod::MostRecentlyUpdated => b.cmp(a),
    }
}
