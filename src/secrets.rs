use std::collections::BTreeMap;

use rand::{Rng, distr::Alphanumeric, rng};

#[derive(Debug, Clone, Copy, PartialEq, Eq, clap::ValueEnum)]
pub enum StackPreset {
    Postgres,
    Redis,
    Jwt,
    Stripe,
    AwsS3,
}

pub fn generate_secret_like(key: &str) -> String {
    let key = key.to_ascii_uppercase();
    let len = if key.contains("JWT") || key.contains("SECRET") || key.contains("TOKEN") {
        48
    } else if key.contains("PASSWORD") {
        32
    } else {
        24
    };

    rng()
        .sample_iter(&Alphanumeric)
        .take(len)
        .map(char::from)
        .collect()
}

pub fn generate_from_presets(presets: &[StackPreset]) -> BTreeMap<String, String> {
    let mut vars = BTreeMap::new();

    for preset in presets {
        match preset {
            StackPreset::Postgres => {
                vars.entry("DB_HOST".to_string())
                    .or_insert_with(|| "localhost".to_string());
                vars.entry("DB_PORT".to_string())
                    .or_insert_with(|| "5432".to_string());
                vars.entry("DB_NAME".to_string())
                    .or_insert_with(|| "app".to_string());
                vars.entry("DB_USER".to_string())
                    .or_insert_with(|| "app".to_string());
                vars.entry("DB_PASSWORD".to_string())
                    .or_insert_with(|| generate_secret_like("DB_PASSWORD"));
            }
            StackPreset::Redis => {
                vars.entry("REDIS_HOST".to_string())
                    .or_insert_with(|| "localhost".to_string());
                vars.entry("REDIS_PORT".to_string())
                    .or_insert_with(|| "6379".to_string());
                vars.entry("REDIS_PASSWORD".to_string())
                    .or_insert_with(|| generate_secret_like("REDIS_PASSWORD"));
            }
            StackPreset::Jwt => {
                vars.entry("JWT_SECRET".to_string())
                    .or_insert_with(|| generate_secret_like("JWT_SECRET"));
            }
            StackPreset::Stripe => {
                vars.entry("STRIPE_SECRET_KEY".to_string())
                    .or_insert_with(|| generate_secret_like("STRIPE_SECRET_KEY"));
            }
            StackPreset::AwsS3 => {
                vars.entry("AWS_ACCESS_KEY_ID".to_string())
                    .or_insert_with(|| generate_secret_like("AWS_ACCESS_KEY_ID"));
                vars.entry("AWS_SECRET_ACCESS_KEY".to_string())
                    .or_insert_with(|| generate_secret_like("AWS_SECRET_ACCESS_KEY"));
                vars.entry("AWS_REGION".to_string())
                    .or_insert_with(|| "us-east-1".to_string());
            }
        }
    }

    vars
}

#[cfg(test)]
mod tests {
    use super::{StackPreset, generate_from_presets, generate_secret_like};

    #[test]
    fn secret_lengths_match_expected_profile() {
        assert_eq!(generate_secret_like("JWT_SECRET").len(), 48);
        assert_eq!(generate_secret_like("DB_PASSWORD").len(), 32);
        assert_eq!(generate_secret_like("API_KEY").len(), 24);
    }

    #[test]
    fn presets_expand_variables() {
        let vars = generate_from_presets(&[StackPreset::Postgres, StackPreset::Jwt]);
        assert!(vars.contains_key("DB_PASSWORD"));
        assert!(vars.contains_key("JWT_SECRET"));
    }
}
