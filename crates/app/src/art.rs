//! Card artwork: Steam's local library images, a custom image copied into the
//! launcher's data folder, or the game's own icon. Nothing is downloaded.

use std::collections::HashMap;
use std::path::PathBuf;

use modlist_core::config::{Artwork, DataDir, Installation, PlayAction};
use modlist_core::mo2::Mo2Install;
use modlist_core::steam::SteamRoot;
use slint::Image;
use uuid::Uuid;

#[derive(Clone, Default)]
pub struct Art {
    pub hero: Option<Image>,
    pub logo: Option<Image>,
    pub icon: Option<Image>,
}

#[derive(Default)]
pub struct ArtCache {
    entries: HashMap<Uuid, (ArtKey, Art)>,
}

#[derive(Clone, PartialEq)]
struct ArtKey {
    artwork: Artwork,
    app_id: Option<u32>,
    icon_source: Option<PathBuf>,
}

fn icon_source(inst: &Installation, install: Option<&Mo2Install>) -> Option<PathBuf> {
    let install = install?;
    let play_title = match &inst.play_action {
        PlayAction::Executable { title } => Some(title.as_str()),
        PlayAction::OpenMo2 => install.default_play_executable().map(|e| e.title.as_str()),
    };
    Some(install.icon_source(play_title))
}

impl ArtCache {
    pub fn get(
        &mut self,
        inst: &Installation,
        install: Option<&Mo2Install>,
        steam: Option<&SteamRoot>,
        data: &DataDir,
    ) -> Art {
        let key = ArtKey {
            artwork: inst.artwork.clone(),
            app_id: inst.steam_app_id,
            icon_source: icon_source(inst, install),
        };
        if let Some((k, art)) = self.entries.get(&inst.id)
            && *k == key
        {
            return art.clone();
        }
        let art = load(&key, steam, data);
        self.entries.insert(inst.id, (key, art.clone()));
        art
    }

    pub fn forget(&mut self, id: Uuid) {
        self.entries.remove(&id);
    }
}

fn load(key: &ArtKey, steam: Option<&SteamRoot>, data: &DataDir) -> Art {
    let image = |p: Option<PathBuf>| p.and_then(|p| Image::load_from_path(&p).ok());
    match &key.artwork {
        Artwork::None => Art::default(),
        Artwork::Custom { file } => Art { hero: image(data.resolve(file).ok()), ..Art::default() },
        Artwork::Automatic => {
            let (hero, logo) = match (steam, key.app_id) {
                (Some(s), Some(app)) => {
                    (image(s.library_image(app, "library_hero.jpg")), image(s.library_image(app, "logo.png")))
                }
                _ => (None, None),
            };
            let icon = if hero.is_none() {
                key.icon_source.as_deref().and_then(|p| crate::platform::icons::exe_icon(p, 256))
            } else {
                None
            };
            Art { hero, logo, icon }
        }
    }
}
