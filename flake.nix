{
  description = "Rustrepo-sanitizer";
  inputs = {
    nixpkgs.url = "github:NixOS/nixpkgs/nixos-24.11";
    flake-utils.url = "github:numtide/flake-utils";
  };
  outputs = { self, nixpkgs, flake-utils }:
    flake-utils.lib.eachSystem [ "x86_64-linux" "aarch64-linux" ] (system:
      let pkgs = import nixpkgs { inherit system; };
      in { packages.default = pkgs.rustPlatform.buildRustPackage {
        pname = "rustrepo-sanitizer";
        version = "0.3.2";
        src = ./.;
        cargoLock.lockFile = ./Cargo.lock;
        meta = { description = "Create deterministic, sanitized AI review bundles from Git repositories";
          homepage = "https://git.itsulu.com/itsulu/Rustrepo-sanitizer";
          license = pkgs.lib.licenses.asl20; };
      }; });
}
