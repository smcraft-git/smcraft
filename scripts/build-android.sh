#!/bin/sh
# Build SMCraft for Android (arm64, API 28+): the Rust library (crates/android) in a NativeActivity
# APK. No Java code and no Gradle: aapt2, zipalign and apksigner from the SDK's build-tools, signed
# with a local debug key (build/android-debug.keystore, made on first use).
#   scripts/build-android.sh                  → build/SMCraft.apk
#   scripts/build-android.sh --install        also adb install it (ADB_SERIAL=<serial> picks the device)
#   scripts/build-android.sh --push <dir>...  copy asset-pack folders (smm2-<build>, smm2-ui-<build>)
#                                             to the device's data folder (adb), then exit
# Needs ANDROID_HOME (the SDK, with an NDK under ndk/), a JDK (JAVA_HOME or keytool/java on PATH) and
# the Rust target: rustup target add aarch64-linux-android.
# The APK holds no game data: the player imports their own dump on the device, or pushes an import.
set -eu
root=$(cd "$(dirname "$0")/.." && pwd)
PKG=${SMCRAFT_ANDROID_PACKAGE:-org.smcraft.smcraft}
API=28
SDK=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-}}
[ -n "$SDK" ] || { echo "set ANDROID_HOME to the Android SDK" >&2; exit 1; }
ADB=${ADB:-$SDK/platform-tools/adb}
adb() { "$ADB" ${ADB_SERIAL:+-s "$ADB_SERIAL"} "$@"; }

if [ "${1:-}" = --push ]; then
    shift
    dest=/sdcard/Android/data/$PKG/files/assets
    adb shell mkdir -p "$dest"
    for d in "$@"; do adb push "$d" "$dest/"; done
    exit 0
fi

NDK=${ANDROID_NDK:-$(ls -d "$SDK"/ndk/* | sort -V | tail -1)}
BT=$(ls -d "$SDK"/build-tools/* | sort -V | tail -1)
JAR=$(ls -d "$SDK"/platforms/android-* | sort -V | tail -1)/android.jar
TC=$(ls -d "$NDK"/toolchains/llvm/prebuilt/* | head -1)/bin
JAVA_BIN=${JAVA_HOME:+$JAVA_HOME/bin/}
for f in "$TC/aarch64-linux-android$API-clang" "$BT/aapt2" "$BT/zipalign" "$BT/apksigner" "$JAR"; do
    [ -e "$f" ] || { echo "build-android: missing $f" >&2; exit 1; }
done

export CARGO_TARGET_AARCH64_LINUX_ANDROID_LINKER="$TC/aarch64-linux-android$API-clang"
export CC_aarch64_linux_android="$TC/aarch64-linux-android$API-clang"
export AR_aarch64_linux_android="$TC/llvm-ar"
(cd "$root/core" && cargo build --release --target aarch64-linux-android -p smcraft-android)
lib=$root/core/target/aarch64-linux-android/release/libsmcraft.so

name=$(sed -n 's/.*static let name = "\(.*\)".*/\1/p' "$root/app/Sources/MMKApp/Identity.swift" | head -1)
version=$(sed -n 's/.*static let version = "\(.*\)".*/\1/p' "$root/app/Sources/MMKApp/Identity.swift" | head -1)
W=$root/build/android
rm -rf "$W"
mkdir -p "$W/root/lib/arm64-v8a"
cat > "$W/AndroidManifest.xml" <<EOF
<?xml version="1.0" encoding="utf-8"?>
<manifest xmlns:android="http://schemas.android.com/apk/res/android" package="$PKG"
    android:versionCode="$(( $(date +%s) / 60 ))" android:versionName="${version:-0.1.0}">
    <uses-sdk android:minSdkVersion="$API" android:targetSdkVersion="35" />
    <uses-feature android:name="android.hardware.vulkan.version" android:version="0x401000" android:required="true" />
    <uses-feature android:name="android.hardware.touchscreen" android:required="false" />
    <uses-feature android:name="android.hardware.gamepad" android:required="false" />
    <uses-permission android:name="android.permission.INTERNET" />
    <application android:label="${name:-SMCraft}" android:hasCode="false" android:isGame="true"
        android:appCategory="game" android:hardwareAccelerated="true" android:extractNativeLibs="false">
        <activity android:name="android.app.NativeActivity" android:exported="true"
            android:screenOrientation="sensorLandscape" android:launchMode="singleTask"
            android:theme="@android:style/Theme.NoTitleBar.Fullscreen"
            android:configChanges="orientation|screenSize|screenLayout|smallestScreenSize|keyboard|keyboardHidden|navigation|uiMode">
            <meta-data android:name="android.app.lib_name" android:value="smcraft" />
            <intent-filter>
                <action android:name="android.intent.action.MAIN" />
                <category android:name="android.intent.category.LAUNCHER" />
            </intent-filter>
        </activity>
    </application>
</manifest>
EOF
"$BT/aapt2" link -o "$W/base.apk" --manifest "$W/AndroidManifest.xml" -I "$JAR"
cp "$lib" "$W/root/lib/arm64-v8a/"
"$TC/llvm-strip" --strip-debug "$W/root/lib/arm64-v8a/libsmcraft.so"
# native libraries stored uncompressed (mapped straight from the APK)
(cd "$W/root" && zip -q -0 -r "$W/base.apk" lib)
"$BT/zipalign" -f -p 16 "$W/base.apk" "$W/aligned.apk"
ks=$root/build/android-debug.keystore
if [ ! -f "$ks" ]; then
    "${JAVA_BIN}keytool" -genkeypair -keystore "$ks" -storepass android -keypass android \
        -alias debug -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=SMCraft debug" >/dev/null
fi
apk=$root/build/SMCraft.apk
"$BT/apksigner" sign --ks "$ks" --ks-pass pass:android --key-pass pass:android \
    --out "$apk" "$W/aligned.apk"
echo "built $apk ($(du -h "$apk" | cut -f1))"
if [ "${1:-}" = --install ]; then
    adb install -r "$apk"
fi
