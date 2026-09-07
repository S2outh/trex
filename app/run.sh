if [[ -v FLASH_LOCAL ]]; then
  exec probe-rs run --chip STM32H723VG "$@"
else
  exec ./../target/release/trex-probe run "$@"
fi
