TARGET_DIR=$(cargo metadata --format-version 1 | jq -r .target_directory)
TARGET_ARCH=thumbv7em-none-eabihf
trex-probe attach $TARGET_DIR/$TARGET_ARCH/debug/trex
