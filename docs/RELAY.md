# Relay product matrix

Canonical rules: `wawona-linux-vms-relay-runtime`, `wawona-guest-wayland-iland`,
`wawona-relay-wasm`, `wawona-mode-a-b`.

Wawona Machines sends `{ kind, image, artifact-class }`. Relay picks the
backend. Kind is `vm` | `container` | `wasm`. Artifact-class is which
binary the user installed (Mode A store/Play vs Mode B tipa/Sileo/desktop-host/root).

Containers always mean: unpack OCI, then the same Linux VM backend as `vm`.
