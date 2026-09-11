if [[ -v FLASH_LOCAL ]]; then
  exec probe-rs run --chip STM32H723VG "$@"
else
  exec trex-probe run --base 0x08040000 --max-size 0x00040000 "$@"
fi
