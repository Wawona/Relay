# wearOS: no VM engine (COMPLIANCE.md). Registry anchor only.
{ pkgs, lib ? pkgs.lib, ... }:
pkgs.writeTextDir "README" ''
  wwn-vms: no VM on wearOS. Use Android phone/tablet VM/container lanes for execution.
''
