//! The runtime accepts the packs this importer writes, and no older ones
//! (`mmk_host::paths::GAME_PACK_VERSION` / `UI_PACK_VERSION` move with the importer).

#[test]
fn the_runtime_reads_packs_from_this_importer() {
    assert_eq!(
        mmk_import::IMPORTER_VERSION,
        mmk_host::paths::GAME_PACK_VERSION
    );
    assert_eq!(
        mmk_import::smm2_ui::SMM2_UI_IMPORTER_VERSION,
        mmk_host::paths::UI_PACK_VERSION
    );
    assert_eq!(
        mmk_import::smm1::SMM1_IMPORTER_VERSION,
        mmk_host::paths::SMM1_PACK_VERSION
    );
}
