{
  inputs = {
    nixpkgs.url = "github:nixos/nixpkgs?ref=nixos-unstable";
  };

  outputs = { self, nixpkgs }: 
  let 
    pkgs = nixpkgs.legacyPackages.x86_64-linux;
  in
  {
    devShells.x86_64-linux.default = pkgs.mkShell {
      buildInputs = [
        (pkgs.python3.withPackages (python-pkgs: [
          python-pkgs.pytest
          python-pkgs.deepdiff
        ]))
      ];
    };
  };
}
