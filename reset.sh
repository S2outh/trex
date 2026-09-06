
MAGIC_WORD="LEMMINGE"
RESET_CMD=3
IP_ADDR=${IP_ADDR:-192.168.0.10}
PORT=${PORT:-3000}
# idle timeout, the device resets right after applying so it never replies
NC_TIMEOUT=${NC_TIMEOUT:-10}

put_u8() {
  printf "\\x$(printf '%02x' "$(( $1 & 0xff ))")"
}

emit_stream() {
  printf '%s' "$MAGIC_WORD"
  put_u8 "$RESET_CMD"
}

if emit_stream | nc -w "$NC_TIMEOUT" "$IP_ADDR" "$PORT"; then
  echo "reset: reset cmd sent" >&2
else
  echo "reset: nc exited non-zero (expected when the device resets on apply)" >&2
fi
