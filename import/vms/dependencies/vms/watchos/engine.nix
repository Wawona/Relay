# watchOS: no VM engine (COMPLIANCE.md). Registry anchor only.
{ pkgs, lib ? pkgs.lib, ... }:
pkgs.writeTextDir "README" ''
  wwn-vms: no VM on watchOS. OCI image management (wwn-containers) may be exposed;
  execution requires iPhone/iPad/Mac/Android targets.
''
