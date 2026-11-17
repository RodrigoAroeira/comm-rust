#!/usr/bin/env sh
# Print "<chip> <port>" for an attached board, or exit 1.
#
# chip is one of: s3 (ESP32-S3), esp32 (classic ESP32).
# Port order defaults to the S3 native USB port, then the common ESP32 bridge.
# Override with PORT=/dev/ttyXXX.

set -eu

if [ -n "${PORT:-}" ]; then
	set -- "$PORT"
else
	set -- /dev/ttyACM0 /dev/ttyUSB0
fi

for port in "$@"; do
	# Only touch device nodes that actually exist: probing a missing port
	# makes espflash wait for a timeout.
	[ -c "$port" ] || continue

	if out=$(espflash board-info --port "$port" 2>&1); then
		line=$(printf '%s\n' "$out" | grep -i 'chip type' | tr '[:upper:]' '[:lower:]')
		case "$line" in
			*esp32-s3* | *esp32s3*)
				echo "s3 $port"
				exit 0
				;;
			*"esp32 ("*)
				echo "esp32 $port"
				exit 0
				;;
		esac
		echo "detect-chip: unsupported chip on $port: ${line:-<no 'Chip type' line>}" >&2
		exit 1
	fi
done

echo "detect-chip: no board found on: $*" >&2
exit 1
