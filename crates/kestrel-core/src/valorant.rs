//! Where VALORANT lives on disk, read from Riot's own install records.

/// Riot's install record for VALORANT.
pub const PRODUCT_SETTINGS: &str =
    "C:/ProgramData/Riot Games/Metadata/valorant.live/valorant.live.product_settings.yaml";
/// Where the Riot Client installs VALORANT by default.
pub const DEFAULT_INSTALL: &str = "C:/Riot Games/VALORANT/live";
const GAME_EXE_RELATIVE: &str = "ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe";

/// Reads `product_install_full_path` from the product settings file. Returns forward slashes.
pub fn install_path_from_product_settings(yaml: &str) -> Option<String> {
    yaml.lines().find_map(|line| {
        let value = line.trim().strip_prefix("product_install_full_path:")?;
        let path = value
            .trim()
            .trim_matches(|c| c == '"' || c == '\'')
            .replace('\\', "/");
        (!path.is_empty()).then_some(path)
    })
}

/// "D:/Games/…" → "D".
pub fn drive_letter(path: &str) -> Option<String> {
    let mut chars = path.chars();
    let letter = chars.next()?;
    (letter.is_ascii_alphabetic() && chars.next() == Some(':'))
        .then(|| letter.to_ascii_uppercase().to_string())
}

/// Full path of the game's executable inside an install folder.
pub fn game_exe_path(install: &str) -> String {
    format!("{}/{GAME_EXE_RELATIVE}", install.trim_end_matches('/'))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_standard_install_path() {
        let yaml = "product_install_full_path: \"C:/Riot Games/VALORANT/live\"\nproduct_install_root: \"C:/Riot Games\"\n";
        assert_eq!(
            install_path_from_product_settings(yaml).as_deref(),
            Some("C:/Riot Games/VALORANT/live")
        );
    }

    #[test]
    fn reads_a_custom_path_with_backslashes_and_single_quotes() {
        let yaml = "  product_install_full_path: 'D:\\Games\\Riot Games\\VALORANT\\live'\n";
        assert_eq!(
            install_path_from_product_settings(yaml).as_deref(),
            Some("D:/Games/Riot Games/VALORANT/live")
        );
    }

    #[test]
    fn missing_or_empty_path_is_none() {
        assert_eq!(
            install_path_from_product_settings("product_install_root: \"C:/Riot Games\""),
            None
        );
        assert_eq!(
            install_path_from_product_settings("product_install_full_path: \"\""),
            None
        );
    }

    #[test]
    fn drive_letters() {
        assert_eq!(drive_letter("d:/Games/VALORANT").as_deref(), Some("D"));
        assert_eq!(drive_letter("relative/path"), None);
        assert_eq!(drive_letter(""), None);
    }

    #[test]
    fn game_exe_path_is_built_from_the_install_folder() {
        assert_eq!(
            game_exe_path("C:/Riot Games/VALORANT/live/"),
            "C:/Riot Games/VALORANT/live/ShooterGame/Binaries/Win64/VALORANT-Win64-Shipping.exe"
        );
    }
}
