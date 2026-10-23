mod browser;
mod csv;
mod xlsx;

pub(crate) use browser::{download, download_url, encoded_bytes};
pub(crate) use csv::encode_table;
pub use xlsx::encode_table_xlsx;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExportFormat {
    Csv,
    Xlsx,
}
impl ExportFormat {
    pub fn from_filename(name: &str) -> Result<Self, String> {
        valid_name(name)?;
        match name.trim().rsplit_once('.') {
            Some((base, ext)) if !base.is_empty() && ext.eq_ignore_ascii_case("csv") => {
                Ok(Self::Csv)
            }
            Some((base, ext)) if !base.is_empty() && ext.eq_ignore_ascii_case("xlsx") => {
                Ok(Self::Xlsx)
            }
            _ => Err("Use a filename ending in .csv or .xlsx".into()),
        }
    }

    pub fn filename(self, name: &str) -> Result<String, String> {
        valid_name(name)?;
        let name = name.trim();
        let base = if name.contains('.') {
            Self::from_filename(name)?;
            name.rsplit_once('.').map_or(name, |(base, _)| base)
        } else {
            name
        };
        Ok(format!("{base}.{}", self.extension()))
    }

    pub fn mime(self) -> &'static str {
        match self {
            Self::Csv => "text/csv;charset=utf-8",
            Self::Xlsx => "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        }
    }

    pub fn extension(self) -> &'static str {
        match self {
            Self::Csv => "csv",
            Self::Xlsx => "xlsx",
        }
    }
}

fn valid_name(name: &str) -> Result<(), String> {
    if name.trim().is_empty() || name.contains(['/', '\\']) || name.chars().any(char::is_control) {
        return Err("Enter a filename without folders or control characters".into());
    }
    Ok(())
}
#[cfg(test)]
#[path = "tests/export.rs"]
pub(crate) mod tests;
