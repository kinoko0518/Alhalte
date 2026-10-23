{
  alhalte,
}:
{
  config,
  lib,
  pkgs,
  ...
}:

let
  cfg = config.services.alhalte;
  system = pkgs.stdenv.hostPlatform.system;
  inherit (lib)
    concatMap
    literalExpression
    mkEnableOption
    mkIf
    mkOption
    optional
    types
    unique
    ;

  defaultPackage = alhalte.packages.${system}.alhalte;
  defaultWebRoot = alhalte.packages.${system}.argenterie-spa;
  isLoopbackHost =
    host:
    host == "localhost"
    || host == "::1"
    || host == "[::1]"
    || (lib.hasPrefix "127." host);

  publicUrl =
    if cfg.publicUrl != null then
      cfg.publicUrl
    else if !isLoopbackHost cfg.domain && cfg.domain != "" then
      "https://${cfg.domain}"
    else
      "http://127.0.0.1:${toString cfg.maitreDPort}";

  loopbackDev = isLoopbackHost cfg.host || isLoopbackHost cfg.domain;
  corsOrigins = unique (
    [
      publicUrl
      "http://${cfg.domain}"
      "https://${cfg.domain}"
      "http://${cfg.domain}:${toString cfg.maitreDPort}"
      "https://${cfg.domain}:${toString cfg.maitreDPort}"
    ]
    ++ optional loopbackDev "http://127.0.0.1:5173"
    ++ optional loopbackDev "http://localhost:5173"
  );

  maitreDArgs =
    [
      "--host"
      cfg.host
      "--port"
      (toString cfg.maitreDPort)
      "--db-path"
      "${cfg.dataDir}/maitre-d.sqlite3"
      "--salt"
      cfg.salt
      "--public-url"
      publicUrl
      "--socket-dir"
      cfg.socketDir
      "--cellar-url"
      "http://127.0.0.1:${toString cfg.cellarPort}"
      "--chunk-size-exponent"
      (toString cfg.chunkSizeExponent)
      "--banquette-heartbeat-interval-secs"
      (toString cfg.heartbeatIntervalSecs)
      "--web-root"
      "${cfg.webRoot}"
    ]
    ++ optional cfg.localLogin "--local-login"
    ++ optional (cfg.clientId != "") "--client-id"
    ++ optional (cfg.clientId != "") cfg.clientId
    ++ optional (cfg.clientSecretFile != null) "--client-secret-file"
    ++ optional (cfg.clientSecretFile != null) (toString cfg.clientSecretFile)
    ++ optional (cfg.serviceTokenFile != null) "--service-token-file"
    ++ optional (cfg.serviceTokenFile != null) (toString cfg.serviceTokenFile)
    ++ concatMap (origin: [ "--cors-origin" origin ]) corsOrigins
    ++ optional cfg.lan "--allow-lan-origin-port"
    ++ optional cfg.lan (toString cfg.maitreDPort);

  cellarArgs = [
    "--host"
    "127.0.0.1"
    "--port"
    (toString cfg.cellarPort)
    "--storage-path"
    "${cfg.dataDir}/cellar/tin"
    "--db-path"
    "${cfg.dataDir}/cellar/cellar.sqlite3"
    "--maitre-d-url"
    "http://127.0.0.1:${toString cfg.maitreDPort}"
  ] ++ optional cfg.antiOracle "--anti-oracle"
    ++ optional (cfg.serviceTokenFile != null) "--service-token-file"
    ++ optional (cfg.serviceTokenFile != null) (toString cfg.serviceTokenFile);
in
{
  options.services.alhalte = {
    enable = mkEnableOption "Alhalte self-hosted VTT services";

    package = mkOption {
      type = types.package;
      default = defaultPackage;
      defaultText = literalExpression "alhalte.packages.${system}.alhalte";
      description = "Alhalte package providing alhalte-maitre-d, alhalte-cellar, and alhalte-banquette.";
    };

    webRoot = mkOption {
      type = types.path;
      default = defaultWebRoot;
      defaultText = literalExpression "alhalte.packages.${system}.argenterie-spa";
      description = "Directory containing the built SPA (index.html, assets, extensions).";
    };

    domain = mkOption {
      type = types.str;
      default = "localhost";
      description = "Public domain of this restaurant instance.";
    };

    host = mkOption {
      type = types.str;
      default = "127.0.0.1";
      description = "Bind address for alhalte-maitre-d. Use 0.0.0.0 to expose the UI/API on the LAN.";
    };

    maitreDPort = mkOption {
      type = types.port;
      default = 8080;
      description = "Port for alhalte-maitre-d and the bundled SPA.";
    };

    cellarPort = mkOption {
      type = types.port;
      default = 8081;
      description = "Port for alhalte-cellar. cellar always binds 127.0.0.1.";
    };

    dataDir = mkOption {
      type = types.path;
      default = "/var/lib/alhalte";
      description = "Persistent data directory (SQLite databases, cellar tin storage, secrets).";
    };

    socketDir = mkOption {
      type = types.path;
      default = "/run/alhalte";
      description = "Directory for banquette UNIX sockets.";
    };

    user = mkOption {
      type = types.str;
      default = "alhalte";
      description = "System user running the Alhalte services.";
    };

    group = mkOption {
      type = types.str;
      default = "alhalte";
      description = "System group running the Alhalte services.";
    };

    localLogin = mkOption {
      type = types.bool;
      default = false;
      description = "Enable the local password login instead of Google OAuth.";
    };

    clientId = mkOption {
      type = types.str;
      default = "";
      description = "Google OAuth client ID. Required unless localLogin is enabled.";
    };

    clientSecretFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "File containing the Google OAuth client secret (mode 0600).";
    };

    salt = mkOption {
      type = types.str;
      default = "";
      description = "Instance-specific salt for account sub hashing.";
    };

    serviceTokenFile = mkOption {
      type = types.nullOr types.path;
      default = null;
      description = "File containing the internal cellar service token (mode 0600).";
    };

    antiOracle = mkOption {
      type = types.bool;
      default = false;
      description = "Enable cellar anti-oracle mode.";
    };

    chunkSizeExponent = mkOption {
      type = types.ints.between 1 30;
      default = 8;
      description = "Log chunk size exponent forwarded to banquette.";
    };

    heartbeatIntervalSecs = mkOption {
      type = types.ints.positive;
      default = 240;
      description = "Room persistence interval in seconds.";
    };

    lan = mkOption {
      type = types.bool;
      default = false;
      description = "Accept private-network browser origins and optionally open the firewall.";
    };

    openFirewall = mkOption {
      type = types.bool;
      default = true;
      description = "Open maitreDPort when lan is enabled.";
    };

    publicUrl = mkOption {
      type = types.nullOr types.str;
      default = null;
      description = "Public base URL for OAuth callbacks. Derived from domain when omitted.";
    };
  };

  config = mkIf cfg.enable {
    assertions = [
      {
        assertion = cfg.salt != "";
        message = "services.alhalte.salt must not be empty.";
      }
      {
        assertion = cfg.localLogin || (cfg.clientId != "" && cfg.clientSecretFile != null);
        message = "services.alhalte.clientId and clientSecretFile are required unless localLogin is enabled.";
      }
    ];

    users.users.${cfg.user} = {
      isSystemUser = true;
      group = cfg.group;
      home = cfg.dataDir;
      createHome = false;
    };
    users.groups.${cfg.group} = { };

    systemd.tmpfiles.rules = [
      "d ${cfg.dataDir} 0700 ${cfg.user} ${cfg.group} -"
      "d ${cfg.socketDir} 0700 ${cfg.user} ${cfg.group} -"
    ];

    systemd.targets.alhalte = {
      description = "Alhalte Restaurant Suite Target";
      wantedBy = [ "multi-user.target" ];
      wants = [
        "alhalte-cellar.service"
        "alhalte-maitre-d.service"
      ];
    };

    systemd.services.alhalte-cellar = {
      description = "Alhalte Cellar File Storage and Tin Service";
      wantedBy = [ "alhalte.target" ];
      partOf = [ "alhalte.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/alhalte-cellar ${lib.escapeShellArgs cellarArgs}";
        Restart = "on-failure";
        RestartSec = "5s";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
      };
    };

    systemd.services.alhalte-maitre-d = {
      description = "Alhalte Maitre-d Authentication and WebSocket Gateway";
      wantedBy = [ "alhalte.target" ];
      partOf = [ "alhalte.target" ];
      after = [ "network.target" ];
      serviceConfig = {
        Type = "simple";
        ExecStart = "${cfg.package}/bin/alhalte-maitre-d ${lib.escapeShellArgs maitreDArgs}";
        Restart = "on-failure";
        RestartSec = "5s";
        User = cfg.user;
        Group = cfg.group;
        WorkingDirectory = cfg.dataDir;
      };
    };

    networking.firewall.allowedTCPPorts = lib.optionals (cfg.lan && cfg.openFirewall) [
      cfg.maitreDPort
    ];
  };
}
