if [[ -v FLASH_LOCAL ]]; then
  exec probe-rs run --chip STM32H723VG "$@"
else
  WORKSPACE_ROOT=$(cargo metadata --format-version 1 | jq -r .workspace_root)
  exec $WORKSPACE_ROOT/target/x86_64-unknown-linux-gnu/release/trex-probe run --base 0x08040000 --max-size 0x00040000 "$@"
fi
