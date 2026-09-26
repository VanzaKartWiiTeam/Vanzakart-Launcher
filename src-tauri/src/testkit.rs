//! Utilità condivise dai test dei servizi.
//!
//! Esiste per una ragione sola: un'installazione finta deve somigliare a
//! quella vera. Finché i test scrivevano un `<wiidisc/>` vuoto al posto del
//! descrittore Riivolution, verificavano uno stato che nella realtà avrebbe
//! fatto partire Mario Kart Wii originale.

use vk_core::ModLayout;

/// Descrittore Riivolution minimo ma valido per una sezione.
///
/// Ha la stessa forma di quello pubblicato dal server: una sezione con le tre
/// opzioni che il launcher attiva e almeno una patch fuori da `<options>`.
pub fn riivolution_xml(section: &str) -> String {
    format!(
        r#"<wiidisc version="1">
    <id game="RMC"/>
    <options>
        <section name="{section}">
            <option name="Pack">
                <choice name="Enabled"><patch id="Load"/></choice>
            </option>
            <option name="My Stuff">
                <choice name="From CTGP-r"><patch id="CTGPLoad"/></choice>
                <choice name="From Pack"><patch id="Load"/></choice>
            </option>
            <option name="Seperate Savegame">
                <choice name="Enabled"><patch id="Save"/></choice>
            </option>
        </section>
    </options>
    <patch id="Load">
        <folder external="/{section}/Binaries" disc="/Binaries" create="true"/>
    </patch>
    <patch id="Save">
        <savegame external="/{section}_UserData/save" clone="true"/>
    </patch>
</wiidisc>"#
    )
}

/// Crea sul disco un'installazione della modpack che supera i controlli
/// d'avvio.
pub fn install_modpack(layout: &ModLayout) {
    let xml = layout.riivolution_xml();
    std::fs::create_dir_all(xml.parent().expect("l'XML ha una directory padre")).unwrap();
    std::fs::write(xml, riivolution_xml(layout.directory_name())).unwrap();
}

/// Un "Dolphin" finto ma vivo: una copia di questo binario di test, con un
/// nome unico, che dorme finché qualcuno non la chiude.
///
/// Il nome unico è ciò che rende il test sicuro: chiudere "Dolphin.exe" per
/// nome, in una CI o sul computer di chi sviluppa, chiuderebbe quello vero.
/// Restituisce il percorso della copia e il processo avviato.
pub fn spawn_fake_dolphin(dir: &std::path::Path) -> (std::path::PathBuf, std::process::Child) {
    let unique = format!(
        "vk-fake-dolphin-{}-{}{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |elapsed| elapsed.as_nanos()),
        std::env::consts::EXE_SUFFIX
    );
    let copy = dir.join(unique);
    std::fs::copy(std::env::current_exe().unwrap(), &copy).unwrap();

    let child = std::process::Command::new(&copy)
        .args([
            "--ignored",
            "--exact",
            "platform::tests::sleeper_for_the_terminate_test",
            "--test-threads=1",
        ])
        .env("VK_TERMINATE_SLEEPER", "1")
        .stdout(std::process::Stdio::null())
        .stderr(std::process::Stdio::null())
        .spawn()
        .unwrap();

    // Il processo deve risultare in esecuzione prima che il test prosegua.
    let started = std::time::Instant::now();
    while !crate::platform::is_executable_running(&copy) {
        assert!(
            started.elapsed() < std::time::Duration::from_secs(10),
            "il processo di prova non è mai partito"
        );
        std::thread::sleep(std::time::Duration::from_millis(50));
    }

    (copy, child)
}

/// Sostituisce il descrittore con un `<wiidisc/>` vuoto: sintatticamente
/// valido, completamente inerte. È il guasto osservato sul campo.
pub fn break_modpack(layout: &ModLayout) {
    let xml = layout.riivolution_xml();
    std::fs::create_dir_all(xml.parent().expect("l'XML ha una directory padre")).unwrap();
    std::fs::write(xml, b"<wiidisc/>").unwrap();
}
