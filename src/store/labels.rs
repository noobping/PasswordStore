use std::path::Path;

pub fn shortened_store_labels(stores: &[String]) -> Vec<String> {
    let path_segments = stores
        .iter()
        .map(|store| {
            Path::new(store)
                .components()
                .filter_map(|component| component.as_os_str().to_str())
                .filter(|segment| !segment.is_empty() && *segment != "/")
                .map(str::to_string)
                .collect::<Vec<_>>()
        })
        .collect::<Vec<_>>();

    let max_depth = path_segments.iter().map(Vec::len).max().unwrap_or_default();
    for depth in 1..=max_depth {
        let labels = path_segments
            .iter()
            .zip(stores)
            .map(|(segments, full_path)| {
                if segments.is_empty() {
                    return full_path.clone();
                }

                let start = segments.len().saturating_sub(depth);
                let suffix = segments[start..].join("/");
                if start == 0 {
                    suffix
                } else {
                    format!(".../{suffix}")
                }
            })
            .collect::<Vec<_>>();

        let mut unique = labels.clone();
        unique.sort();
        unique.dedup();
        if unique.len() == labels.len() {
            return labels;
        }
    }

    stores.to_vec()
}

#[cfg(test)]
mod tests {
    use super::shortened_store_labels;

    #[test]
    fn store_labels_use_short_unique_suffixes() {
        let stores = vec![
            "/home/nick/.password-store".to_string(),
            "/home/nick/work/.password-store".to_string(),
        ];

        assert_eq!(
            shortened_store_labels(&stores),
            vec![
                ".../nick/.password-store".to_string(),
                ".../work/.password-store".to_string(),
            ]
        );
    }

    #[test]
    fn store_labels_fall_back_to_full_paths_when_needed() {
        let stores = vec!["/same".to_string(), "/same".to_string()];

        assert_eq!(shortened_store_labels(&stores), stores);
    }
}
