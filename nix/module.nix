# NixOS module: `services.curatarr`.
#
# Renders /etc/curatarr/curatarr.toml from the options below, runs
# `curatarr serve` as a hardened systemd service with a dedicated system
# user, and registers `rootFolders` in the database at startup. Anything
# not covered by a dedicated option goes into `settings`, which is merged
# into the TOML verbatim (see curatarr.example.toml for the keys).
{ config, lib, pkgs, ... }:
let
  cfg = config.services.curatarr;
  toml = pkgs.formats.toml { };

  rootFolderToml = f: {
    path = f.path;
    content_types = f.contentTypes;
  } // lib.optionalAttrs (f.name != null) { name = f.name; };

  generated = {
    server = {
      host = cfg.host;
      port = cfg.port;
    };
    database = {
      backend = "sqlite";
      url = "sqlite://${cfg.dataDir}/curatarr.db?mode=rwc";
    };
    library = {
      data_dir = cfg.dataDir;
      root_folders = map rootFolderToml cfg.rootFolders;
    };
    log = {
      level = cfg.logLevel;
      format = "json";
    };
  };

  settings = lib.recursiveUpdate generated cfg.settings;
  configFile = toml.generate "curatarr.toml" settings;

  rootFolderPaths = map (f: f.path) cfg.rootFolders;
in
{
  options.services.curatarr = {
    enable = lib.mkEnableOption "curatarr, the ebook/comic/manga acquisition manager";

    package = lib.mkPackageOption pkgs "curatarr" { };

    user = lib.mkOption {
      type = lib.types.str;
      default = "curatarr";
      description = "System user the service runs as. Created when it is the default.";
    };

    group = lib.mkOption {
      type = lib.types.str;
      default = "curatarr";
      description = "Primary group of the service user. Created when it is the default.";
    };

    supplementaryGroups = lib.mkOption {
      type = lib.types.listOf lib.types.str;
      default = [ ];
      example = [ "media" ];
      description = "Extra groups, typically the one that owns the library directories.";
    };

    host = lib.mkOption {
      type = lib.types.str;
      default = "0.0.0.0";
      description = "Address to listen on.";
    };

    port = lib.mkOption {
      type = lib.types.port;
      default = 8787;
      description = "HTTP port. Note that Readarr also defaults to 8787.";
    };

    openFirewall = lib.mkOption {
      type = lib.types.bool;
      default = false;
      description = "Open `port` in the firewall.";
    };

    dataDir = lib.mkOption {
      type = lib.types.path;
      default = "/var/lib/curatarr";
      description = "Database, cover cache (`covers/`) and recycle bin (`recycle/`).";
    };

    umask = lib.mkOption {
      type = lib.types.str;
      default = "0022";
      example = "0002";
      description = "UMask for files the service creates. Use 0002 for group-writable libraries.";
    };

    logLevel = lib.mkOption {
      type = lib.types.str;
      default = "info";
      description = "tracing filter, e.g. `info` or `curatarr_scanner=debug,info`.";
    };

    rootFolders = lib.mkOption {
      default = [ ];
      description = ''
        Library directories registered at startup. The service gets read/write
        access to each and waits for its mount. Use `services.curatarr.supplementaryGroups`
        to grant group access when the directories belong to another user.
      '';
      example = lib.literalExpression ''
        [
          { path = "/srv/books"; name = "Books"; contentTypes = [ "book" ]; }
          { path = "/srv/comics"; contentTypes = [ "comic" "manga" ]; }
        ]
      '';
      type = lib.types.listOf (lib.types.submodule {
        options = {
          path = lib.mkOption {
            type = lib.types.path;
            description = "Absolute path of the library directory.";
          };
          name = lib.mkOption {
            type = lib.types.nullOr lib.types.str;
            default = null;
            description = "Display name.";
          };
          contentTypes = lib.mkOption {
            type = lib.types.listOf (lib.types.enum [
              "book"
              "comic"
              "manga"
              "graphic_novel"
              "light_novel"
              "webtoon"
            ]);
            default = [ ];
            description = "Content types expected in this folder (informational for now).";
          };
        };
      });
    };

    settings = lib.mkOption {
      type = lib.types.submodule { freeformType = toml.type; };
      default = { };
      example = lib.literalExpression ''
        {
          library.naming_template = "{AuthorSort}/{Title} ({Year}).{Extension}";
          library.import_mode = "hardlink";
          library.recycle_retention_days = 14;
        }
      '';
      description = ''
        Extra configuration merged over the generated `curatarr.toml`.
        See curatarr.example.toml for every key.
      '';
    };
  };

  config = lib.mkIf cfg.enable {
    environment.systemPackages = [ cfg.package ];
    environment.etc."curatarr/curatarr.toml".source = configFile;

    users.users = lib.mkIf (cfg.user == "curatarr") {
      curatarr = {
        isSystemUser = true;
        group = cfg.group;
        home = cfg.dataDir;
        description = "curatarr service user";
      };
    };

    users.groups = lib.mkIf (cfg.group == "curatarr") {
      curatarr = { };
    };

    networking.firewall.allowedTCPPorts = lib.mkIf cfg.openFirewall [ cfg.port ];

    systemd.tmpfiles.rules = lib.mkIf (cfg.dataDir != "/var/lib/curatarr") [
      "d ${cfg.dataDir} 0750 ${cfg.user} ${cfg.group} -"
    ];

    systemd.services.curatarr = {
      description = "curatarr - ebook, comic and manga acquisition manager";
      wantedBy = [ "multi-user.target" ];
      after = [ "network-online.target" ];
      wants = [ "network-online.target" ];
      unitConfig.RequiresMountsFor = rootFolderPaths;
      restartTriggers = [ configFile ];

      serviceConfig = {
        Type = "simple";
        User = cfg.user;
        Group = cfg.group;
        SupplementaryGroups = cfg.supplementaryGroups;
        UMask = cfg.umask;
        ExecStart = "${lib.getExe cfg.package} --config /etc/curatarr/curatarr.toml serve";
        Restart = "on-failure";
        RestartSec = 5;
        WorkingDirectory = cfg.dataDir;
        StateDirectory = lib.mkIf (cfg.dataDir == "/var/lib/curatarr") "curatarr";
        StateDirectoryMode = "0750";

        # Hardening. The service needs the network (for later metadata
        # providers), its data directory and the library directories,
        # nothing else.
        NoNewPrivileges = true;
        PrivateTmp = true;
        PrivateDevices = true;
        ProtectSystem = "strict";
        ProtectHome = true;
        ProtectKernelTunables = true;
        ProtectKernelModules = true;
        ProtectKernelLogs = true;
        ProtectControlGroups = true;
        ProtectClock = true;
        ProtectHostname = true;
        ProtectProc = "invisible";
        RestrictAddressFamilies = [ "AF_INET" "AF_INET6" "AF_UNIX" ];
        RestrictNamespaces = true;
        RestrictRealtime = true;
        RestrictSUIDSGID = true;
        LockPersonality = true;
        MemoryDenyWriteExecute = true;
        SystemCallArchitectures = "native";
        SystemCallFilter = [ "@system-service" "~@privileged" "~@resources" ];
        CapabilityBoundingSet = "";
        ReadWritePaths = [ cfg.dataDir ] ++ rootFolderPaths;
      };
    };
  };
}
