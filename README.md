# `hos-rv` for `mljboard`

Clients running [`mljboard-client`](https://github.com/duckfromdiscord/mljboard-client) will connect to this server to expose a HTTP server on their network to [`mljboard`](https://github.com/duckfromdiscord/mljboard). `hos-rv` optionally only allows connections from localhost (with command line flag `-b`) so that only a local `mljboard` can access and query it. It's recommended to run with `-b` for safety, especially since `hos-rv` deals with pairing codes.

## Optional shuttle integration

**Compile with `--no-default-features` to disable shuttle integration.**

Create a `Secrets.toml` following the example in `Secrets.toml.example`. The only thing you'll need is a `HOS_PASSWD`.