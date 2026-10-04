# Keep this import: Relay owns the guest's devices and Wayland transport.
{ relayModule, ... }:
{
  imports = [ relayModule ];
}
