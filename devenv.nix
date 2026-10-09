{
  pkgs,
  lib,
  config,
  inputs,
  ...
}: {
  languages.rust = {
    enable = true;
    toolchainFile = ./rust-toolchain.toml;
  };

  # https://devenv.sh/packages/
  packages = with pkgs; [
    git
    claude-code

    # Native toolchain for the Rust workspace's -sys crates.
    cmake
    pkg-config
    protobuf # iota-network / starfish codegen
    libpq.dev # pq-sys (iota-indexer, iota-graphql-rpc)
    udev.dev # hidapi (iota-ledger)
  ];

  # https://devenv.sh/basics/
  env = {
    GREET = "devenv";

    # librocksdb-sys runs bindgen. Only the shared library is exported: putting
    # llvmPackages.libclang on PATH would shadow the wrapped clang that cc-rs
    # needs to find the libc headers.
    LIBCLANG_PATH = "${pkgs.llvmPackages.libclang.lib}/lib";
    PROTOC = "${pkgs.protobuf}/bin/protoc";
    PROTOC_INCLUDE = "${pkgs.protobuf}/include";
    PQ_LIB_DIR = "${pkgs.libpq}/lib";
  };

  # https://devenv.sh/languages/
  # languages.rust.enable = true;

  # https://devenv.sh/processes/
  # processes.dev.exec = "${lib.getExe pkgs.watchexec} -n -- ls -la";

  # https://devenv.sh/services/
  # services.postgres.enable = true;

  # https://devenv.sh/scripts/
  scripts.hello.exec = ''
    echo hello from $GREET
  '';

  # https://devenv.sh/basics/
  enterShell = ''
    hello         # Run scripts directly
    git --version # Use packages
  '';

  # https://devenv.sh/tasks/
  # tasks = {
  #   "myproj:setup".exec = "mytool build";
  #   "devenv:enterShell".after = [ "myproj:setup" ];
  # };

  # https://devenv.sh/tests/
  enterTest = ''
    echo "Running tests"
    git --version | grep --color=auto "${pkgs.git.version}"
  '';

  # https://devenv.sh/git-hooks/
  # git-hooks.hooks.shellcheck.enable = true;

  # See full reference at https://devenv.sh/reference/options/
}
