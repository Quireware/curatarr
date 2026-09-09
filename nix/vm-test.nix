# NixOS VM test for the module: the service starts under the hardening
# profile, serves the API, registers the configured root folder, and can
# scan a group-owned library directory in place.
{ pkgs, module }:
pkgs.testers.nixosTest {
  name = "curatarr-nixos-module";

  nodes.machine = { ... }: {
    imports = [ module ];

    users.groups.media = { };

    services.curatarr = {
      enable = true;
      port = 8790;
      openFirewall = true;
      supplementaryGroups = [ "media" ];
      umask = "0002";
      rootFolders = [
        { path = "/srv/books"; name = "Books"; contentTypes = [ "book" ]; }
      ];
      settings.library.naming_template = "{Author}/{Title}.{Extension}";
    };

    # A library directory owned by another user but group-writable, like a
    # shared media tree. The fake PDF is enough for format detection.
    systemd.tmpfiles.rules = [
      "d /srv/books 2775 root media -"
      "f /srv/books/Sample.pdf 0664 root media - %PDF-1.4"
      "d /srv/inbox 2775 root media -"
      "f /srv/inbox/Incoming.pdf 0664 root media - %PDF-1.4"
    ];

    environment.systemPackages = [ pkgs.curl pkgs.jq ];
  };

  testScript = ''
    import json

    start_all()
    machine.wait_for_unit("curatarr.service")
    machine.wait_for_open_port(8790)

    api = "http://127.0.0.1:8790/api/v1"

    machine.succeed("curl --fail --silent http://127.0.0.1:8790/health | grep -F '\"ok\"'")
    machine.succeed("curl --fail --silent http://127.0.0.1:8790/health/ready | grep -F '\"ready\"'")

    # Config landed where the unit expects it and the service user exists.
    machine.succeed("test -f /etc/curatarr/curatarr.toml")
    machine.succeed("grep -F '/srv/books' /etc/curatarr/curatarr.toml")
    machine.succeed("id curatarr")
    machine.succeed("id -nG curatarr | tr ' ' '\\n' | grep -Fx media")
    machine.succeed("test -d /var/lib/curatarr")

    # Root folder from config was registered at startup.
    folders = json.loads(machine.succeed(f"curl --fail --silent {api}/root-folders"))
    assert len(folders) == 1, folders
    assert folders[0]["path"] == "/srv/books", folders
    assert folders[0]["accessible"] is True, folders
    folder_id = folders[0]["id"]

    # In-place scan finds the sample file.
    machine.succeed(f"curl --fail --silent -X POST {api}/root-folders/{folder_id}/scan")
    machine.wait_until_succeeds(
        f"curl --fail --silent {api}/root-folders/{folder_id}/scan/status | jq -e '.state == \"completed\"'"
    )
    status = json.loads(machine.succeed(f"curl --fail --silent {api}/root-folders/{folder_id}/scan/status"))
    assert status["imported"] == 1, status
    works = json.loads(machine.succeed(f"curl --fail --silent {api}/works"))
    assert works["total"] == 1, works
    assert works["items"][0]["title"] == "Sample", works
    machine.succeed("test -f /srv/books/Sample.pdf")

    # Organised import into the group-owned tree works through the umask/group setup.
    summary = json.loads(machine.succeed(
        f"curl --fail --silent -X POST -H 'content-type: application/json' "
        f"-d '{{\"source\": \"/srv/inbox\"}}' {api}/root-folders/{folder_id}/import"
    ))
    assert summary["imported"] == 1, summary
    machine.succeed("test -f '/srv/books/Unknown Author/Incoming.pdf'")
    machine.succeed("test \"$(stat -c '%U:%G' '/srv/books/Unknown Author')\" = 'curatarr:media'")

    # Hardening is in effect: no write outside the allowed paths.
    machine.fail("systemd-run --wait --pipe -p User=curatarr touch /etc/curatarr-should-fail")
  '';
}
