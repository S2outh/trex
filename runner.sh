#!/usr/bin/env bash
# cargo runner, see .cargo/config.toml -- cargo hands us the linked ELF.
#
#   FLASH_LOCAL=1 cargo run   flash over the debug probe with probe-rs
#   cargo run                 push the image to the update server of the
#                             running firmware over TCP (see app/src/update.rs)
set -eu

if [[ -v FLASH_LOCAL ]]; then
  exec probe-rs run --chip STM32H723VG "$@"
fi

ELF=${1:?usage: runner.sh <elf>}

MAGIC_WORD="LEMMINGE"
VALIDATE_CMD=0
CHUNK_CMD=1
APPLY_CMD=2
CHUNK_SIZE=4096
IP_ADDR=${IP_ADDR:-192.168.0.10}
PORT=${PORT:-3000}
# idle timeout, the device resets right after applying so it never replies
NC_TIMEOUT=${NC_TIMEOUT:-10}

# Raw little endian integers on stdout. usize is 4 bytes on thumbv7em, which
# is what update.rs reads for the offset/size fields.
put_u8() {
  printf "\\x$(printf '%02x' "$(( $1 & 0xff ))")"
}
put_u32() {
  local v=$1
  printf "\\x$(printf '%02x' "$(( v & 0xff ))")"
  printf "\\x$(printf '%02x' "$(( (v >> 8) & 0xff ))")"
  printf "\\x$(printf '%02x' "$(( (v >> 16) & 0xff ))")"
  printf "\\x$(printf '%02x' "$(( (v >> 24) & 0xff ))")"
}

# The firmware image has to travel through files and dd the whole way: bash
# variables cannot hold NUL bytes, so `read`/`echo` would silently mangle it.
emit_stream() {
  local off=0 len

  while (( off < SIZE )); do
    len=$(( SIZE - off ))
    if (( len > CHUNK_SIZE )); then
      len=$CHUNK_SIZE
    fi

    printf '%s' "$MAGIC_WORD"
    put_u8 "$CHUNK_CMD"
    put_u32 "$off"
    put_u32 "$len"
    dd if="$BIN" bs="$CHUNK_SIZE" skip=$(( off / CHUNK_SIZE )) count=1 status=none

    off=$(( off + len ))
  done

  printf '%s' "$MAGIC_WORD"
  put_u8 "$APPLY_CMD"
  put_u32 "$SIZE"
  cat "$HASH"
}

# NOTE: deliberately not $OBJCOPY -- the nix shell already sets that to the
# host gcc objcopy, which cannot read the thumb ELF.
OBJCOPY=${RUNNER_OBJCOPY:-}
if [[ -z $OBJCOPY ]]; then
  for candidate in rust-objcopy llvm-objcopy arm-none-eabi-objcopy; do
    if command -v "$candidate" >/dev/null 2>&1; then
      OBJCOPY=$candidate
      break
    fi
  done
fi
if [[ -z $OBJCOPY ]]; then
  echo "runner: no objcopy in PATH (need cargo-binutils or llvm-tools)" >&2
  exit 1
fi

WORK=$(mktemp -d)
trap 'rm -rf "$WORK"' EXIT
BIN=$WORK/firmware.bin
HASH=$WORK/firmware.b3

# The updater writes what it receives straight to flash, so it needs the raw
# image and not the ELF cargo passes us.
"$OBJCOPY" -O binary "$ELF" "$BIN"

SIZE=$(wc -c < "$BIN")
if (( SIZE == 0 )); then
  echo "runner: $ELF produced an empty image" >&2
  exit 1
fi

# Hash the same bytes the device hashes: the first $SIZE bytes of the dfu
# partition. Everything the device writes past that is chunk padding.
b3sum --no-names --raw "$BIN" > "$HASH"

echo "runner: sending $SIZE bytes to $IP_ADDR:$PORT" >&2
if emit_stream | nc -w "$NC_TIMEOUT" "$IP_ADDR" "$PORT"; then
  echo "runner: image sent, device should be rebooting into it" >&2
else
  echo "runner: nc exited non-zero (expected when the device resets on apply)" >&2
fi
