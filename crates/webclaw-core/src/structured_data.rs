/// Extract JSON-LD structured data from HTML.
///
/// Parses `<script type="application/ld+json">` blocks commonly found in
/// e-commerce, news, and recipe sites. Returns machine-readable product info,
/// prices, availability, reviews, etc. without needing JS rendering or LLM.
use serde_json::Value;

/// Extract all JSON-LD blocks from raw HTML.
///
/// Returns parsed JSON values, skipping any blocks that fail to parse.
/// Most e-commerce sites include Schema.org Product markup with prices,
/// sizes, availability, and images.
pub fn extract_json_ld(html: &str) -> Vec<Value> {
    let mut results = Vec::new();
    let needle = "application/ld+json";

    // Walk through the HTML finding <script type="application/ld+json"> blocks.
    // Using simple string scanning instead of a full HTML parser — these blocks
    // are self-contained and reliably structured.
    let mut search_from = 0;
    while let Some(tag_start) = html[search_from..].find("<script") {
        let abs_start = search_from + tag_start;
        let tag_region = &html[abs_start..];

        // Find the end of the opening tag
        let Some(tag_end_offset) = tag_region.find('>') else {
            search_from = abs_start + 7;
            continue;
        };

        let opening_tag = &tag_region[..tag_end_offset];

        // Check if this is a JSON-LD script
        if !opening_tag.to_lowercase().contains(needle) {
            search_from = abs_start + tag_end_offset + 1;
            continue;
        }

        // Find the closing </script>
        let content_start = abs_start + tag_end_offset + 1;
        let remaining = &html[content_start..];
        let Some(close_offset) = remaining.to_lowercase().find("</script>") else {
            search_from = content_start;
            continue;
        };

        let json_str = remaining[..close_offset].trim();
        search_from = content_start + close_offset + 9;

        if json_str.is_empty() {
            continue;
        }

        // Parse — some sites have arrays at top level
        match serde_json::from_str::<Value>(json_str) {
            Ok(Value::Array(arr)) => results.extend(arr),
            Ok(val) => results.push(val),
            Err(_) => {}
        }
    }

    results
}

/// Extract JSON-like objects from regular <script> tags.
///
/// This handles modern frameworks like SvelteKit that embed data as JS object
/// literals (where keys aren't quoted) rather than pure JSON.
pub fn extract_js_objects(html: &str) -> Vec<Value> {
    let mut results = Vec::new();

    // Look for SvelteKit's kit.start({ ... data: [...] }) pattern
    // or general large JS object assignments.
    let mut search_from = 0;
    while let Some(tag_start) = html[search_from..].find("<script") {
        let abs_start = search_from + tag_start;
        let tag_region = &html[abs_start..];

        let Some(tag_end_offset) = tag_region.find('>') else {
            search_from = abs_start + 7;
            continue;
        };

        let opening_tag = &tag_region[..tag_end_offset].to_lowercase();
        // Skip JSON-LD (already handled) or external scripts
        if opening_tag.contains("application/ld+json") || opening_tag.contains("src=") {
            search_from = abs_start + tag_end_offset + 1;
            continue;
        }

        let content_start = abs_start + tag_end_offset + 1;
        let remaining = &html[content_start..];
        let Some(close_offset) = remaining.to_lowercase().find("</script>") else {
            search_from = content_start;
            continue;
        };

        let js_code = &remaining[..close_offset];
        search_from = content_start + close_offset + 9;

        // Try to find the "data" array in SvelteKit initialization
        // Uses a regex to handle data: [ or data:[ or data:  [
        let data_re = regex::Regex::new(r"data\s*:\s*\[").unwrap();
        if let Some(mat) = data_re.find(js_code) {
            let start_idx = mat.start() + (mat.as_str().find('[').unwrap());
            if let Some(data_array) = extract_balanced_bracket(&js_code[start_idx..], '[', ']') {
                if let Ok(val) = parse_js_literal(&data_array) {
                    if let Value::Array(arr) = val {
                        for item in arr {
                            if !item.is_null() {
                                results.push(item);
                            }
                        }
                    } else {
                        results.push(val);
                    }
                }
            }
        }
    }

    results
}

/// Extract content between balanced brackets.
fn extract_balanced_bracket(text: &str, open: char, close: char) -> Option<String> {
    let mut depth = 0;
    let mut start = None;

    for (i, c) in text.char_indices() {
        if c == open {
            if depth == 0 {
                start = Some(i);
            }
            depth += 1;
        } else if c == close {
            depth -= 1;
            if depth == 0 {
                if let Some(s) = start {
                    return Some(text[s..=i].to_string());
                }
            }
        }
    }
    None
}

/// Attempt to parse a JavaScript object literal as JSON by quoting keys.
/// This is a heuristic parser for data islands.
fn parse_js_literal(js: &str) -> Result<Value, serde_json::Error> {
    // 1. Try direct parse (sometimes it IS valid JSON)
    if let Ok(v) = serde_json::from_str(js) {
        return Ok(v);
    }

    // 2. heuristic: wrap keys in quotes and remove JS specific values
    let mut cleaned = js.to_string();

    // Replace unquoted keys: {key: or ,key: -> {"key": or ,"key":
    // Handles whitespace around the key and colon
    let key_regex = regex::Regex::new(r#"([{,])\s*([a-zA-Z_][a-zA-Z0-9_]*)\s*:"#).unwrap();
    cleaned = key_regex.replace_all(&cleaned, r#"$1"$2":"#).to_string();

    // Remove "new URL(...)" wrappers - greedy matching for the constructor
    let url_regex = regex::Regex::new(r"new\s+URL\s*\([^)]+\)").unwrap();
    cleaned = url_regex.replace_all(&cleaned, "\"redacted_url\"").to_string();

    // Remove "undefined"
    cleaned = cleaned.replace("undefined", "null");

    serde_json::from_str(&cleaned)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn extracts_single_json_ld() {
        let html = r#"
            <html><head>
            <script type="application/ld+json">{"@type":"Product","name":"Test"}</script>
            </head><body></body></html>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["@type"], "Product");
        assert_eq!(results[0]["name"], "Test");
    }

    #[test]
    fn extracts_multiple_json_ld_blocks() {
        let html = r#"
            <script type="application/ld+json">{"@type":"WebSite","url":"https://example.com"}</script>
            <script type="application/ld+json">{"@type":"Product","name":"Shoe","offers":{"price":99.99}}</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 2);
        assert_eq!(results[0]["@type"], "WebSite");
        assert_eq!(results[1]["@type"], "Product");
    }

    #[test]
    fn handles_array_json_ld() {
        let html = r#"
            <script type="application/ld+json">[{"@type":"BreadcrumbList"},{"@type":"Product"}]</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 2);
    }

    #[test]
    fn skips_invalid_json() {
        let html = r#"
            <script type="application/ld+json">{invalid json here}</script>
            <script type="application/ld+json">{"@type":"Product","name":"Valid"}</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "Valid");
    }

    #[test]
    fn ignores_regular_script_tags() {
        let html = r#"
            <script>console.log("not json-ld")</script>
            <script type="text/javascript">var x = 1;</script>
            <script type="application/ld+json">{"@type":"Product"}</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn handles_no_json_ld() {
        let html = "<html><body><p>No structured data here</p></body></html>";
        let results = extract_json_ld(html);
        assert!(results.is_empty());
    }

    #[test]
    fn case_insensitive_type() {
        let html = r#"
            <script type="Application/LD+JSON">{"@type":"Product"}</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
    }

    #[test]
    fn handles_whitespace_in_json() {
        let html = r#"
            <script type="application/ld+json">
                {
                    "@type": "Product",
                    "name": "Test"
                }
            </script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
        assert_eq!(results[0]["name"], "Test");
    }

    #[test]
    fn empty_script_tag_skipped() {
        let html = r#"
            <script type="application/ld+json">   </script>
            <script type="application/ld+json">{"@type":"Product"}</script>
        "#;
        let results = extract_json_ld(html);
        assert_eq!(results.len(), 1);
    }
}
