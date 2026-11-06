//! Strings of the native parts (tray menu, toasts). The panel has its own dictionary in `ui/i18n.js`.
//! French is the default language; English is the alternative.

pub fn tr(lang: &str, key: &'static str) -> &'static str {
    let en = lang == "en";
    match key {
        "tray.open" => if en { "Open Fixatoutix" } else { "Ouvrir Fixatoutix" },
        "tray.nail" => if en { "Nail the active window" } else { "Clouer la fenêtre active" },
        "tray.gather" => if en { "Gather! (put everything back)" } else { "Rassemblement ! (tout remettre en place)" },
        "tray.guard" => if en { "On guard" } else { "En garde" },
        "tray.banquet" => if en { "At the banquet (resume guard)" } else { "Au banquet (reprendre la garde)" },
        "tray.autostart" => if en { "Start with Windows" } else { "Lancer au démarrage de Windows" },
        "tray.update" => if en { "Update available: install" } else { "Mise à jour disponible : installer" },
        "tray.quit" => if en { "Quit" } else { "Quitter" },
        "tip.guard" => if en { "Fixatoutix: on guard" } else { "Fixatoutix : en garde" },
        "tip.banquet" => if en { "Fixatoutix: at the banquet" } else { "Fixatoutix : au banquet" },
        "toast.nailed1" => if en { "By Toutatis, it's nailed!" } else { "Par Toutatis, c'est cloué !" },
        "toast.nailed2" => if en { "Solid as a menhir." } else { "Solide comme un menhir." },
        "toast.nailed3" => if en { "Well forged!" } else { "Bien forgé !" },
        "toast.renailed" => if en { "By Toutatis, it's nailed again!" } else { "Par Toutatis, c'est recloué !" },
        "toast.gathered" => if en { "Gather!" } else { "Rassemblement !" },
        "toast.gathered.sub" => if en { "Everyone back in place." } else { "Tout le monde à sa place." },
        "toast.resists" => if en { "It resists…" } else { "Elle résiste…" },
        "toast.resists.window" => if en { "No window to nail here." } else { "Pas de fenêtre à clouer ici." },
        "toast.resists.state" => if en { "Minimized or maximized windows cannot be nailed." } else { "Une fenêtre minimisée ou maximisée ne se cloue pas." },
        "toast.resists.nowindow" => if en { "No open window for this rule." } else { "Aucune fenêtre ouverte pour cette règle." },
        "toast.resists.shortcut" => if en { "This shortcut is already taken." } else { "Ce raccourci est déjà pris." },
        "toast.banquet" => if en { "At the banquet" } else { "Au banquet" },
        "toast.banquet.sub" => if en { "Nothing moves until you are back on guard." } else { "Plus rien ne bouge avant le retour en garde." },
        "toast.guard" => if en { "On guard" } else { "En garde" },
        "toast.guard.sub" => if en { "Windows are back in place." } else { "Les fenêtres sont remises en place." },
        "toast.update" => if en { "A new Fixatoutix is available" } else { "Un nouveau Fixatoutix est prêt" },
        "toast.update.sub" => if en { "Open the panel to install it." } else { "Ouvre le panneau pour l'installer." },
        "toast.updated" => if en { "Updated!" } else { "Mise à jour faite !" },
        "toast.config" => if en { "Config unreadable" } else { "Config illisible" },
        "toast.config.sub" => if en { "Saved as .bak, starting fresh." } else { "Copiée en .bak, on repart de zéro." },
        "toast.autostart.fail" => if en { "Cannot change autostart." } else { "Impossible de changer le démarrage automatique." },
        _ => "",
    }
}

/// One of the three "nailed" toasts, rotating.
pub fn nailed_variant(lang: &str) -> &'static str {
    let n = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.subsec_nanos())
        .unwrap_or(0)
        % 3;
    tr(lang, ["toast.nailed1", "toast.nailed2", "toast.nailed3"][n as usize])
}
