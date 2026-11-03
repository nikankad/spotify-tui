//! Configuration files are small; reject excessive YAML expansion and nesting.
use serde::Deserialize;

pub fn from_str<'de, T: Deserialize<'de>>(input: &'de str) -> Result<T, serde_yaml::Error> {
  let options = serde_yaml::options! {
    budget: serde_yaml::budget! {
      max_depth: 16,
      max_nodes: 8192,
      max_events: 32768,
      max_aliases: 128,
      max_anchors: 128,
      max_documents: 1,
      max_total_scalar_bytes: 1024 * 1024,
      max_recorded_anchor_bytes: 1024 * 1024,
      max_recorded_anchor_events: 8192,
    },
  };
  serde_yaml::from_str_with_options(input, options)
}

#[cfg(test)]
mod tests {
  #[test]
  fn rejects_deeply_nested_and_duplicate_configuration() {
    let nested = format!("{}0{}", "[".repeat(100), "]".repeat(100));
    assert!(super::from_str::<serde_json::Value>(&nested).is_err());
    assert!(super::from_str::<serde_json::Value>("key: first\nkey: second\n").is_err());
  }
  #[test]
  fn limits_alias_expansion() {
    let aliases = format!("[&a value{}]", ", *a".repeat(129));
    assert!(super::from_str::<serde_json::Value>(&aliases).is_err());
  }
}
