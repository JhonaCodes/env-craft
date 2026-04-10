pub fn sanitize_segment(value: &str) -> String {
    value
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() {
                ch.to_ascii_uppercase()
            } else {
                '_'
            }
        })
        .collect::<String>()
        .trim_matches('_')
        .split('_')
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>()
        .join("_")
}

pub fn vault_secret_name(project: &str, environment: &str, key: &str) -> String {
    format!(
        "{}_{}_{}",
        sanitize_segment(project),
        sanitize_segment(environment),
        sanitize_segment(key)
    )
}

#[cfg(test)]
mod tests {
    use super::{sanitize_segment, vault_secret_name};

    #[test]
    fn sanitizes_segments_to_upper_snake_case() {
        assert_eq!(sanitize_segment("nui-app"), "NUI_APP");
        assert_eq!(sanitize_segment("prod-eu-west"), "PROD_EU_WEST");
        assert_eq!(sanitize_segment("JWT__secret"), "JWT_SECRET");
    }

    #[test]
    fn builds_full_secret_name() {
        assert_eq!(
            vault_secret_name("nui-app", "prod", "db.password"),
            "NUI_APP_PROD_DB_PASSWORD"
        );
    }
}
