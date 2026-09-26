//! Build dell'installer.
//!
//! Su Windows l'installer chiede i permessi di amministratore già all'avvio,
//! dal manifest: installa in Programmi e registra il programma per tutto il PC
//! (§D-090). Solo nelle build di rilascio, però: con il manifest elevato un
//! processo non elevato non può nemmeno avviarlo (errore 740), quindi `tauri
//! dev` e `cargo test` si fermerebbero. `VK_SETUP_AS_INVOKER` produce una build
//! di rilascio che non chiede niente, per provarla senza UAC.
//!
//! La dipendenza da Common-Controls 6 è quella del manifest predefinito di
//! Tauri e deve restare: senza, i dialoghi nativi non si aprono.

const COMMON_CONTROLS: &str = r#"
  <dependency>
    <dependentAssembly>
      <assemblyIdentity
        type="win32"
        name="Microsoft.Windows.Common-Controls"
        version="6.0.0.0"
        processorArchitecture="*"
        publicKeyToken="6595b64144ccf1df"
        language="*"
      />
    </dependentAssembly>
  </dependency>"#;

fn manifest(level: &str) -> String {
    format!(
        r#"<assembly xmlns="urn:schemas-microsoft-com:asm.v1" manifestVersion="1.0">{COMMON_CONTROLS}
  <trustInfo xmlns="urn:schemas-microsoft-com:asm.v3">
    <security>
      <requestedPrivileges>
        <requestedExecutionLevel level="{level}" uiAccess="false" />
      </requestedPrivileges>
    </security>
  </trustInfo>
</assembly>
"#
    )
}

fn main() {
    println!("cargo:rerun-if-env-changed=VK_SETUP_AS_INVOKER");

    let release = std::env::var("PROFILE").is_ok_and(|profile| profile == "release");
    let as_invoker = std::env::var_os("VK_SETUP_AS_INVOKER").is_some();
    let level = if release && !as_invoker {
        "requireAdministrator"
    } else {
        "asInvoker"
    };

    let windows = tauri_build::WindowsAttributes::new().app_manifest(manifest(level));
    tauri_build::try_build(tauri_build::Attributes::new().windows_attributes(windows))
        .expect("build dell'installer non riuscita");
}
