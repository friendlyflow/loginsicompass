//! Drift guards for what the package ships but no compiler checks.

/// The renderer (`sicompass-ui`) compiles the fonts into this binary, so the
/// package has to ship their license texts. `fonts/` keeps copies for
/// `flake.nix` to install, and those have to be the texts of the fonts actually
/// linked.
#[test]
fn shipped_font_licenses_match_the_linked_renderer() {
    assert!(
        !sicompass_ui::fonts::LICENSES.is_empty(),
        "sicompass-ui exposes no font licenses to check"
    );
    for (name, text) in sicompass_ui::fonts::LICENSES {
        let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join("fonts")
            .join(name);
        let shipped = std::fs::read_to_string(&path)
            .unwrap_or_else(|e| panic!("reading {}: {e}", path.display()));
        assert_eq!(
            shipped, *text,
            "fonts/{name} differs from the one in the sicompass-ui version this \
             build links. Copy it over from that repo's fonts/."
        );
    }
}

fn read(path: &str) -> String {
    let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(path);
    std::fs::read_to_string(&full).unwrap_or_else(|e| panic!("reading {}: {e}", full.display()))
}

/// The lines of the table `[name]` in a TOML file.
fn table<'a>(toml: &'a str, name: &str) -> Vec<&'a str> {
    let header = format!("[{name}]");
    toml.lines()
        .skip_while(|l| *l != header)
        .skip(1)
        .take_while(|l| !l.starts_with('['))
        .collect()
}

/// The double-quoted strings on one line.
fn strings(line: &str) -> Vec<&str> {
    line.split('"').skip(1).step_by(2).collect()
}

/// `(source, destination)` of each asset in the .deb's list. Destinations get a
/// leading slash, the way the .rpm's list spells them.
fn deb_assets(toml: &str) -> std::collections::BTreeSet<(String, String)> {
    table(toml, "package.metadata.deb")
        .iter()
        .filter(|l| l.trim_start().starts_with("[\""))
        .map(|l| {
            let s = strings(l);
            (s[0].to_owned(), format!("/{}", s[1]))
        })
        .collect()
}

fn rpm_assets(toml: &str) -> std::collections::BTreeSet<(String, String)> {
    table(toml, "package.metadata.generate-rpm")
        .iter()
        .filter(|l| l.trim_start().starts_with("{ source"))
        .map(|l| {
            let s = strings(l);
            (s[0].to_owned(), s[1].to_owned())
        })
        .collect()
}

/// The two lists in Cargo.toml hold the same files.
#[test]
fn deb_and_rpm_ship_the_same_files() {
    let toml = read("Cargo.toml");
    let deb = deb_assets(&toml);
    let rpm = rpm_assets(&toml);
    assert!(!deb.is_empty() && !rpm.is_empty(), "no assets found");
    assert_eq!(deb, rpm);
}

#[test]
fn packages_ship_the_program_and_its_licenses() {
    let assets = rpm_assets(&read("Cargo.toml"));
    let sources: Vec<&str> = assets.iter().map(|(s, _)| s.as_str()).collect();
    for source in [
        "target/release/loginsicompass",
        "packaging/loginsicompass-session",
        "LICENSE",
        "THIRD-PARTY-LICENSES.html",
        "fonts/LICENSE-*.txt",
    ] {
        assert!(sources.contains(&source), "{source} is not in the packages");
    }
}

#[test]
fn every_packaged_file_exists_in_the_checkout() {
    for (source, _) in rpm_assets(&read("Cargo.toml")) {
        if source.starts_with("target/") || source.contains('*') {
            continue;
        }
        let full = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join(&source);
        assert!(full.is_file(), "{source} is packaged but does not exist");
    }
}

/// Each of these cost someone a silent or a dead login screen on NixOS, where
/// the module in desicompass's flake.nix tells the stories.
#[test]
fn the_session_script_keeps_what_is_load_bearing() {
    let script = read("packaging/loginsicompass-session");
    for word in [
        "XDG_CONFIG_HOME=/var/lib/loginsicompass/xdg/config",
        "XDG_STATE_HOME=/var/lib/loginsicompass/xdg/state",
        "XDG_DATA_HOME=/var/lib/loginsicompass/xdg/data",
        "XDG_CACHE_HOME=/var/lib/loginsicompass/xdg/cache",
        "DESICOMPASS_SUPERKEY=\n",
        "DESICOMPASS_BAR=\n",
        "systemd-cat --identifier=loginsicompass",
        "dbus-run-session",
        "--backend tty",
        "--startup-cmd loginsicompass",
    ] {
        assert!(script.contains(word), "the session script lost `{word}`");
    }
}

/// The greeter writes the accessibility settings it shares with the session,
/// and the state folder holds them until then.
#[test]
fn the_install_script_gives_the_greeter_what_it_writes() {
    let script = read("packaging/scripts/postinst");
    assert!(script.contains("usermod -aG sicompass-a11y"));
    assert!(script.contains("/var/lib/loginsicompass/xdg"));
    // Telling the administrator about the file is fine, touching it is not.
    assert!(
        !script
            .lines()
            .any(|l| l.contains("/etc/greetd") && !l.trim_start().starts_with("echo")),
        "choosing the greeter stays the administrator's step"
    );
}

/// Dlopened, so no scan finds them and the ELF headers do not name them, and no
/// desktop lacks them, so only a clean install (scripts/check-install.sh) ever
/// shows one missing.
#[test]
fn dlopened_libraries_are_declared_in_both_packages() {
    let toml = read("Cargo.toml");
    let deb = table(&toml, "package.metadata.deb").join("\n");
    let rpm = table(&toml, "package.metadata.generate-rpm").join("\n");
    for (package, soname) in [
        ("libwayland-client0", "libwayland-client.so.0"),
        ("libwayland-egl1", "libwayland-egl.so.1"),
        ("libwayland-cursor0", "libwayland-cursor.so.0"),
        ("libvulkan1", "libvulkan.so.1"),
    ] {
        assert!(deb.contains(package), "the .deb should depend on {package}");
        assert!(
            rpm.contains(&format!("\"{soname}()(64bit)\"")),
            "the .rpm should require {soname}"
        );
    }
}

/// The tool's own scan lists what the libraries link too, and Ubuntu's are not
/// Fedora's: that made desicompass's .rpm impossible to install.
#[test]
fn the_rpm_requires_only_what_the_binary_links_directly() {
    let script = read("scripts/release-package.sh");
    assert!(script.contains("--auto-req packaging/rpm-find-requires"));
    assert!(read("packaging/rpm-find-requires").contains("NEEDED"));
}
