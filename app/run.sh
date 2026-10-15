if [[ -v FLASH_LOCAL ]]; then
  exec probe-rs run --chip STM32H723VG "$@"
else
  SCRIPT_DIR=$( cd -- "$( dirname -- "${BASH_SOURCE[0]}" )" &> /dev/null && pwd )
  exec trex-probe -v run --base 0x08040000 --max-size 0x00040000 "$@" "$SCRIPT_DIR/secrets/private.key"
fi
