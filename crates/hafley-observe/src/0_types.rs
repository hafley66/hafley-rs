use std::fmt;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum OutputFormat {
    #[default]
    Human,
    Json,
}

impl OutputFormat {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Human => "human",
            Self::Json => "json",
        }
    }

    pub fn parse(value: &str) -> Result<Self, ParseOutputFormatError> {
        match value {
            "human" | "text" => Ok(Self::Human),
            "json" => Ok(Self::Json),
            value => Err(ParseOutputFormatError(value.to_owned())),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ParseOutputFormatError(pub String);

impl fmt::Display for ParseOutputFormatError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            formatter,
            "unknown log format {:?}; expected human or json",
            self.0
        )
    }
}

impl std::error::Error for ParseOutputFormatError {}

#[derive(Clone, Debug)]
pub struct Config {
    pub service_name: &'static str,
    pub service_version: &'static str,
    pub default_filter: &'static str,
    pub format: OutputFormat,
    pub ansi: bool,
}

impl Config {
    pub fn from_env(
        service_name: &'static str,
        service_version: &'static str,
        default_filter: &'static str,
        ansi: bool,
    ) -> Result<Self, ParseOutputFormatError> {
        let format = match std::env::var("HAFLEY_LOG_FORMAT") {
            Ok(value) => OutputFormat::parse(&value)?,
            Err(std::env::VarError::NotPresent) => OutputFormat::Human,
            Err(std::env::VarError::NotUnicode(value)) => {
                return Err(ParseOutputFormatError(value.to_string_lossy().into_owned()))
            }
        };
        Ok(Self {
            service_name,
            service_version,
            default_filter,
            format,
            ansi,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::OutputFormat;

    #[test]
    fn output_format_vocabulary_is_fixed() {
        let actual = ["human", "text", "json", "pretty"]
            .map(|value| OutputFormat::parse(value).map_err(|error| error.to_string()));
        assert_eq!(
            actual,
            [
                Ok(OutputFormat::Human),
                Ok(OutputFormat::Human),
                Ok(OutputFormat::Json),
                Err("unknown log format \"pretty\"; expected human or json".to_owned()),
            ]
        );
        assert_eq!(
            [OutputFormat::Human.as_str(), OutputFormat::Json.as_str()],
            ["human", "json"]
        );
    }
}
