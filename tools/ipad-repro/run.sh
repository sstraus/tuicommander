#!/bin/sh
# usage: run.sh TAG URL [X Y DX DY HOLD STEPS]   (sim: iPad Pro 13 M5, iOS 26.5)
# Needs: python3 -m http.server 8123 in web/ (started separately), xcui build in dd/.
SIM=${SIM:-BB821790-FFCA-4839-910E-06224D45FD21}
# Scratch (xcodebuild output, logs): TUIC_IPAD_DIR, else the caller's temp dir.
host_tmp=${TMPDIR:-/tmp}; D="${TUIC_IPAD_DIR:-${host_tmp%/}/tuic-1329-ipad}"; export TMPDIR="$D/"
TAG=$1; URL=$2
xcrun simctl bootstatus $SIM -b >/dev/null 2>&1
xcrun simctl openurl $SIM "$URL"; sleep 3
TEST_RUNNER_OUT="$D/out" TEST_RUNNER_TAG="$TAG" TEST_RUNNER_X=${3:-0.5} TEST_RUNNER_Y=${4:-0.7} TEST_RUNNER_DX=${5:-0} TEST_RUNNER_DY=${6:--0.4} TEST_RUNNER_HOLD=${7:-0.05} TEST_RUNNER_STEPS=${8:-1} TEST_RUNNER_DOUBLE=$DOUBLE TEST_RUNNER_TAPX=$TAPX TEST_RUNNER_TAPY=$TAPY \
xcodebuild test-without-building -project "$D/xcui/Swipe.xcodeproj" -scheme SwipeUITests -destination "platform=iOS Simulator,id=$SIM" -derivedDataPath "$D/dd" > "$D/out/$TAG.log" 2>&1
echo exit=$?; grep -E "Test Case|error:|passed|failed" "$D/out/$TAG.log" | tail -5
