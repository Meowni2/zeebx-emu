fn main() {
    // O unicorn-engine-sys traz o cputlb do QEMU, que usa atômicos de 128 bits
    // (__atomic_load_16, __atomic_store_16, __atomic_compare_exchange_16). No
    // x86-64 o compilador não os gera inline sem cmpxchg16b, então eles vivem
    // na libatomic do GCC e precisam ser linkados explicitamente.
    if cfg!(target_os = "linux") && std::env::var_os("CARGO_FEATURE_UNICORN").is_some() {
        // Só o backend QEMU precisa destes atômicos de 128 bits. O core Libretro não habilita
        // unicorn e, portanto, não pode exigir libatomic do sistema do handheld.
        println!("cargo:rustc-link-lib=atomic");
    }
    // O ícone e a versão entram no `.exe`. O `CARGO_CFG_TARGET_OS` é o do alvo, e não o da
    // máquina que compila: o `cfg!` aqui responderia pelo sistema em que o build.rs roda.
    if std::env::var("CARGO_CFG_TARGET_OS").as_deref() == Ok("windows") {
        let mut recurso = winresource::WindowsResource::new();
        recurso.set_icon("assets/icones/zeebx.ico");
        if let Err(erro) = recurso.compile() {
            println!("cargo:warning=sem ícone no .exe: {erro}");
        }
    }
    println!("cargo:rerun-if-changed=assets/icones/zeebx.ico");
    println!("cargo:rerun-if-changed=build.rs");
}
