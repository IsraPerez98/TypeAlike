//! Tells whether the app in front is a game: an app whose `Info.plist` puts it in the Games
//! category of the App Store (`LSApplicationCategoryType`), the key that Game Mode uses, or an app
//! without a category in a Steam library.

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSString, NSURL, ns_string};
use typealike_core::desktop::GameActivity;

const GAMES_CATEGORY: &str = "public.app-category.games";
const CATEGORY_PREFIX: &str = "public.app-category.";
const GAME_GENRE_SUFFIX: &str = "-games";
const STEAM_LIBRARY_GAMES_FOLDER: &str = "/steamapps/common/";

/// Reads whether the app in front is a game.
pub(crate) fn read() -> GameActivity {
    let Some(bundle_url) = NSWorkspace::sharedWorkspace()
        .frontmostApplication()
        .and_then(|application| application.bundleURL())
    else {
        return GameActivity::None;
    };
    let Some(bundle_path) = bundle_url.path() else {
        return GameActivity::None;
    };
    if is_game(&bundle_path.to_string(), app_category(&bundle_url).as_deref()) {
        GameActivity::InFront
    } else {
        GameActivity::None
    }
}

/// The App Store category of the app bundle at `bundle_url`, from its `Info.plist`.
fn app_category(bundle_url: &NSURL) -> Option<String> {
    let bundle = NSBundle::bundleWithURL(bundle_url)?;
    let category = bundle
        .objectForInfoDictionaryKey(ns_string!("LSApplicationCategoryType"))?
        .downcast::<NSString>()
        .ok()?;
    Some(category.to_string())
}

/// Whether the app bundle at `bundle_path` with the App Store `category` is a game. The category
/// decides when the app has one, thus an engine editor from Steam is no game.
fn is_game(bundle_path: &str, category: Option<&str>) -> bool {
    match category {
        Some(category) => is_game_category(category),
        None => bundle_path.contains(STEAM_LIBRARY_GAMES_FOLDER),
    }
}

/// Whether `category` is the Games category or one of its genres, such as
/// `public.app-category.puzzle-games`.
fn is_game_category(category: &str) -> bool {
    category == GAMES_CATEGORY
        || category
            .strip_prefix(CATEGORY_PREFIX)
            .is_some_and(|genre| genre.ends_with(GAME_GENRE_SUFFIX))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn games_category_and_its_genres_are_games() {
        let categories = [
            "public.app-category.games",
            "public.app-category.action-games",
            "public.app-category.role-playing-games",
        ];

        let are_games = categories.map(is_game_category);

        assert_eq!(are_games, [true, true, true]);
    }

    #[test]
    fn other_categories_are_no_games() {
        let categories = [
            "public.app-category.productivity",
            "public.app-category.developer-tools",
            "games",
            "",
        ];

        let are_games = categories.map(is_game_category);

        assert_eq!(are_games, [false, false, false, false]);
    }

    #[test]
    fn app_without_a_category_in_a_steam_library_is_a_game() {
        let bundle_paths = [
            "/Volumes/Games/SteamLibrary/steamapps/common/Some Game/Some Game.app",
            "/Users/Shared/SteamLibrary/steamapps/common/Other Game",
        ];

        let are_games = bundle_paths.map(|bundle_path| is_game(bundle_path, None));

        assert_eq!(are_games, [true, true]);
    }

    #[test]
    fn app_without_a_category_outside_a_steam_library_is_no_game() {
        let bundle_paths = ["/Applications/Some Editor.app", "/Applications/Steam.app"];

        let are_games = bundle_paths.map(|bundle_path| is_game(bundle_path, None));

        assert_eq!(are_games, [false, false]);
    }

    #[test]
    fn category_of_the_app_decides_in_a_steam_library() {
        let bundle_path = "/Volumes/Games/SteamLibrary/steamapps/common/Some Engine/Some Engine.app";

        let is_engine_editor_a_game = is_game(bundle_path, Some("public.app-category.developer-tools"));

        assert!(!is_engine_editor_a_game);
    }
}
