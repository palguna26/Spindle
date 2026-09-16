use serde::{Deserialize, Deserializer, Serialize, Serializer};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) enum PopupSize {
    Cells(u16),
    Percent(u8),
}

impl PopupSize {
    pub(crate) fn resolve(self, available: u16) -> u16 {
        match self {
            Self::Cells(cells) => cells,
            Self::Percent(percent) => ((available as u32 * percent as u32) / 100) as u16,
        }
    }

    pub(crate) fn parse_cli(value: &str) -> Result<Self, String> {
        if let Some(percent) = value.strip_suffix('%') {
            let percent = percent
                .parse::<u8>()
                .map_err(|_| "must be cells or a percentage like 80%".to_string())?;
            if !(1..=100).contains(&percent) {
                return Err("percentage must be between 1% and 100%".to_string());
            }
            return Ok(Self::Percent(percent));
        }
        value
            .parse::<u16>()
            .map(Self::Cells)
            .map_err(|_| "must be cells or a percentage like 80%".to_string())
    }
}

impl Serialize for PopupSize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        match self {
            Self::Cells(cells) => serializer.serialize_u16(*cells),
            Self::Percent(percent) => serializer.serialize_str(&format!("{percent}%")),
        }
    }
}

impl<'de> Deserialize<'de> for PopupSize {
    fn deserialize<D>(deserializer: D) -> Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        struct Visitor;

        impl serde::de::Visitor<'_> for Visitor {
            type Value = PopupSize;

            fn expecting(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                formatter.write_str("a cell count or percentage string like 80%")
            }

            fn visit_u64<E>(self, value: u64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u16::try_from(value)
                    .map(PopupSize::Cells)
                    .map_err(|_| E::custom("cell count must fit in u16"))
            }

            fn visit_i64<E>(self, value: i64) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                u16::try_from(value)
                    .map(PopupSize::Cells)
                    .map_err(|_| E::custom("cell count must be between 0 and 65535"))
            }

            fn visit_str<E>(self, value: &str) -> Result<Self::Value, E>
            where
                E: serde::de::Error,
            {
                PopupSize::parse_cli(value)
                    .and_then(|size| match size {
                        PopupSize::Percent(_) => Ok(size),
                        PopupSize::Cells(_) => {
                            Err("string sizes must be percentages like 80%".to_string())
                        }
                    })
                    .map_err(E::custom)
            }
        }

        deserializer.deserialize_any(Visitor)
    }
}

#[cfg(test)]
mod tests {
    use super::PopupSize;

    #[test]
    fn parses_and_resolves_cells_and_percentages() {
        assert_eq!(PopupSize::parse_cli("90"), Ok(PopupSize::Cells(90)));
        assert_eq!(PopupSize::parse_cli("80%"), Ok(PopupSize::Percent(80)));
        assert_eq!(PopupSize::Percent(80).resolve(100), 80);
    }

    #[test]
    fn rejects_invalid_percentages() {
        assert!(PopupSize::parse_cli("0%").is_err());
        assert!(PopupSize::parse_cli("101%").is_err());
        assert!(PopupSize::parse_cli("%").is_err());
    }

    #[test]
    fn keeps_numeric_manifest_values_compatible() {
        assert_eq!(
            serde_json::from_str::<PopupSize>("90").unwrap(),
            PopupSize::Cells(90)
        );
        assert_eq!(
            serde_json::from_str::<PopupSize>(r#""80%""#).unwrap(),
            PopupSize::Percent(80)
        );
    }
}
