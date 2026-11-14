# Release signing

Postern releases are signed with an Ed25519 keypair. The private key
lives on the build host (`postern-email`) at
`~/.config/postern-release-signing/priv.pem` (mode 600, ubuntu user
only). It never leaves that machine.

`postern-release.pub` in this directory is the matching public key,
embedded into:
- the Rust binary at compile time (`include_bytes!`);
- the host-side updater (`deploy/auto-deploy/postern-updater.sh`).

The post-receive hook on each build server, after producing
`postern-<sha>.tar.gz`, runs

```sh
openssl pkeyutl -sign -rawin \
    -inkey ~/.config/postern-release-signing/priv.pem \
    -in postern-<sha>.tar.gz \
    -out postern-<sha>.tar.gz.sig
```

The `.sig` file is published next to the tarball. Both the Rust client
and the host updater verify the signature against the embedded public
key before trusting the release.

## Rotation

Generate a new keypair, copy `priv.pem` to the build host, replace
`postern-release.pub` here, ship a release. Old releases stay verifiable
using whatever client they originally shipped with — only future
releases will require the new key. If a key is suspected compromised:
revoke by shipping a release that bakes in the new pubkey, then accept
that pre-rotation clients can't verify post-rotation releases until
they update via a different (out-of-band) channel.

## Fingerprint

```
7ed82ede322c0fccbbdfea49e21d4ca620a9b48f9b05b7a5f9b996f8138125bf
```

(SHA-256 of the DER-encoded public key.)
