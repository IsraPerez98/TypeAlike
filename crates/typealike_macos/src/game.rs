//! Tells whether the app in front is a game: an app whose `Info.plist` puts it in the Games
//! category of the App Store (`LSApplicationCategoryType`). macOS uses the same key to find the
//! games for Game Mode.

use objc2_app_kit::NSWorkspace;
use objc2_foundation::{NSBundle, NSString, ns_string};
use typealike_core::desktop::GameActivity;

const GAMES_CATEGORY: &str = "public.app-category.games";
const CATEGORY_PREFIX: &str = "public.app-category.";
const GAME_GENRE_SUFFIX: &str = "-games";

/// Reads whether the app in front is a game.
pub(crate) fn read() -> GameActivity {
    if front_app_category().is_some_and(|category| is_game_category(&category)) {
        GameActivity::InFront
    } else {
        GameActivity::None
    }
}

/// The App Store category of the app in front, from its `Info.plist`.
fn front_app_category() -> Option<String> {
    let application = NSWorkspace::sharedWorkspace().frontmostApplication()?;
    let url = application.bundleURL()?;
    let bundle = NSBundle::bundleWithURL(&url)?;
    let category = bundle
        .objectForInfoDictionaryKey(ns_string!("LSApplicationCategoryType"))?
        .downcast::<NSString>()
        .ok()?;
    Some(category.to_string())
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
}
