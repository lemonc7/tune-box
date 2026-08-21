use dioxus::{core::AttributeValue::Text, prelude::Attribute};

// 合并html attributes
pub(super) fn merge_attributes(mut lists: Vec<Vec<Attribute>>) -> Vec<Attribute> {
    let mut merged = Vec::new();
    for list in &mut lists {
        list.sort_by_key(|attribute| attribute.name);
    }
    let mut iterators: Vec<_> = lists
        .into_iter()
        .map(|list| list.into_iter().peekable())
        .collect();

    loop {
        let minimum_name = iterators
            .iter_mut()
            .filter_map(|iterator| iterator.peek().map(|attribute| attribute.name))
            .min();
        let Some(minimum_name) = minimum_name else {
            break;
        };

        let mut by_namespace: Vec<Attribute> = Vec::new();
        for iterator in &mut iterators {
            while iterator.peek().map(|attribute| attribute.name) == Some(minimum_name) {
                let attribute = iterator.next().expect("peeked attribute must exist");
                if let Some(existing) = by_namespace
                    .iter_mut()
                    .find(|existing| existing.namespace == attribute.namespace)
                {
                    if attribute.name == "class" {
                        let volatile = existing.volatile || attribute.volatile;
                        *existing = match (&existing.value, &attribute.value) {
                            (Text(left), Text(right)) => Attribute {
                                name: attribute.name,
                                namespace: attribute.namespace,
                                volatile,
                                value: Text(join_classes(left, right)),
                            },
                            _ => attribute,
                        };
                    } else {
                        *existing = attribute;
                    }
                } else {
                    by_namespace.push(attribute);
                }
            }
        }
        merged.extend(by_namespace);
    }

    merged
}

fn join_classes(left: &str, right: &str) -> String {
    let (left, right) = (left.trim(), right.trim());
    if !left.is_empty() && !right.is_empty() {
        format!("{left} {right}")
    } else {
        format!("{left}{right}")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn concatenates_class_attributes() {
        let merged = merge_attributes(vec![
            vec![Attribute::new("class", "base", None, false)],
            vec![Attribute::new("class", "caller", None, false)],
        ]);

        assert!(matches!(
            &merged[0].value,
            Text(value) if value == "base caller"
        ));
    }

    #[test]
    fn later_attributes_override_defaults() {
        let merged = merge_attributes(vec![
            vec![Attribute::new("data-size", "default", None, false)],
            vec![Attribute::new("data-size", "lg", None, false)],
        ]);

        assert!(matches!(&merged[0].value, Text(value) if value == "lg"));
    }
}
