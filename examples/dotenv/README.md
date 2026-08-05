# examples/dotenv

A small dotenv loader that exercises Aura's process and string APIs.

It reads `KEY=value` entries, ignores blank lines and comments, supports
optional `export` prefixes and matching single or double quotes, then sets the
values in the current process with `std.os.setEnv`.

## Run

From the repository root:

```bash
cp examples/dotenv/.env.example target/dotenv.env
cargo run -p aura-cli -- run examples/dotenv -- target/dotenv.env
cargo run -p aura-cli -- run examples/dotenv -- target/dotenv.env --get GREETING
```

Expected output includes:

```text
loaded 4 entries from target/dotenv.env
APP_NAME=Aura Dotenv
APP_ENV=development
PORT=8080
```

The environment changes belong to the dotenv process and its descendants; the
parent shell is not modified.

## Layout

| Path            | Role                               |
| --------------- | ---------------------------------- |
| `src/main.aura` | parser, environment loader and CLI |
| `.env.example`  | safe sample configuration          |
