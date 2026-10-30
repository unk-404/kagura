#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Metadata {
    pub flags: Vec<String>,
    pub features: Vec<String>,
    pub includes: Vec<String>,
    pub negative: Option<Negative>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Negative {
    pub phase: String,
    pub kind: String,
}

pub fn parse_metadata(source: &str) -> Metadata {
    let Some(block) = metadata_block(source) else {
        return Metadata::default();
    };
    let lines: Vec<&str> = block.lines().collect();
    let mut metadata = Metadata::default();
    let mut index = 0;
    while index < lines.len() {
        let line = lines[index].trim();
        if let Some(value) = line.strip_prefix("flags:") {
            metadata.flags = parse_list(value);
        } else if let Some(value) = line.strip_prefix("features:") {
            metadata.features = parse_list(value);
        } else if let Some(value) = line.strip_prefix("includes:") {
            metadata.includes = parse_list(value);
        } else if line == "negative:" {
            let mut phase = String::new();
            let mut kind = String::new();
            let mut next = index + 1;
            while next < lines.len() {
                let nested = lines[next].trim();
                if let Some(value) = nested.strip_prefix("phase:") {
                    phase = unquote(value.trim()).to_owned();
                } else if let Some(value) = nested.strip_prefix("type:") {
                    kind = unquote(value.trim()).to_owned();
                } else if !nested.is_empty() {
                    break;
                }
                next += 1;
            }
            metadata.negative = Some(Negative { phase, kind });
            index = next;
            continue;
        }
        index += 1;
    }
    metadata
}

fn metadata_block(source: &str) -> Option<&str> {
    let start = source.find("/*---")? + 5;
    let rest = &source[start..];
    let end = rest.find("---*/")?;
    Some(&rest[..end])
}

fn parse_list(value: &str) -> Vec<String> {
    let value = value.trim();
    let value = value
        .strip_prefix('[')
        .and_then(|value| value.strip_suffix(']'))
        .unwrap_or(value);
    value
        .split(',')
        .map(|item| unquote(item.trim()).to_owned())
        .filter(|item| !item.is_empty())
        .collect()
}

fn unquote(value: &str) -> &str {
    value
        .strip_prefix('"')
        .and_then(|value| value.strip_suffix('"'))
        .or_else(|| {
            value
                .strip_prefix('\'')
                .and_then(|value| value.strip_suffix('\''))
        })
        .unwrap_or(value)
}
