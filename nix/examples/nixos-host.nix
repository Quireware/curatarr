# Example host configuration consuming the curatarr flake.
#
# In the consuming flake:
#
#   inputs.curatarr = {
#     url = "github:Quireware/curatarr";
#     inputs.nixpkgs.follows = "nixpkgs";
#   };
#
#   nixosConfigurations.myhost = nixpkgs.lib.nixosSystem {
#     modules = [
#       curatarr.nixosModules.default
#       ./hosts/myhost/curatarr.nix   # this file
#     ];
#   };
#
# The service runs as the `curatarr` system user. Library directories that
# belong to someone else are reached through `supplementaryGroups` plus a
# group-writable umask, the same pattern the other *arr services use.
{ ... }:
{
  services.curatarr = {
    enable = true;
    # Readarr also listens on 8787; pick another port if both run on the host.
    port = 8788;
    openFirewall = true;

    supplementaryGroups = [ "media" ];
    umask = "0002";

    rootFolders = [
      { path = "/mnt/external/BOOKS"; name = "Books"; contentTypes = [ "book" ]; }
      { path = "/mnt/external/COMICS"; name = "Comics"; contentTypes = [ "comic" "manga" ]; }
    ];

    settings = {
      library.naming_template = "{Author}/{Series}/{SeriesPositionPadded} - {Title}.{Extension}";
      library.import_mode = "copy";
      library.recycle_retention_days = 30;
    };
  };

  # Wait for the library disk before starting.
  systemd.services.curatarr = {
    after = [ "mnt-external.mount" ];
    requires = [ "mnt-external.mount" ];
  };

  users.groups.media.members = [ "curatarr" ];
}
