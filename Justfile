cross_compile := "true"
cargo_build_binary := if cross_compile == "true" { "cross" } else { "cargo" }

[group('deps')]
install-cross:
    cargo install cross --git https://github.com/cross-rs/cross

[group('build')]
[doc('Builds all tempo binaries in cargo release mode')]
build-all-release extra_args="": (_build-release "reth-malachite" extra_args)

[group('build')]
[doc('Builds all tempo binaries')]
build-all extra_args="": (_build "reth-malachite" extra_args)

_build-release target extra_args="": (_build target "-r " + extra_args)

_build target extra_args="":
    CROSS_CONTAINER_IN_CONTAINER=true RUSTFLAGS="-C link-arg=-lgcc -Clink-arg=-static-libgcc" \
        {{cargo_build_binary}} build {{extra_args}} --bin {{target}}
