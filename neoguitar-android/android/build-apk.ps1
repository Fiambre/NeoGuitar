# Builds the Android app without Gradle: cargo-ndk -> aapt2 -> zipalign -> apksigner (and bundletool for the AAB).
# Debug APK (signed with the debug key):      build-apk.ps1 [-Install]
# Play Store bundle (signed with upload key): build-apk.ps1 -Bundle -Keystore <file.jks> [-Alias upload] [-VersionCode N]
#   The keystore password is read from the NEOGUITAR_KS_PASS environment variable.
param([switch]$Install, [switch]$Bundle, [string]$Abi = "arm64-v8a", [string]$Keystore, [string]$Alias = "upload", [int]$VersionCode = 1)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path "$PSScriptRoot\..\.."
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$env:ANDROID_HOME = $sdk
if (-not $env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME = (Get-ChildItem "$sdk\ndk" -Directory | Sort-Object Name | Select-Object -Last 1).FullName }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = (Get-ChildItem "C:\Program Files\Microsoft" -Directory -Filter "jdk-17*" | Select-Object -First 1).FullName }
$bt = (Get-ChildItem "$sdk\build-tools" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$jar = "$sdk\platforms\android-36\android.jar"
$out = "$repo\target\android"
$minSdk = 26      # AAudio (low-latency audio) needs API 26
$targetSdk = 36   # Google Play requires API 36 for new apps since 2026-08-31
$versionName = (Select-String -Path "$repo\neoguitar-android\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value
$bundletool = (Get-ChildItem "$env:LOCALAPPDATA\Android\tools" -Filter "bundletool-all-*.jar" -ErrorAction SilentlyContinue | Select-Object -Last 1).FullName

Push-Location $repo
try {
    # Android 15+ devices use 16 KB memory pages: the native library must be linked for that.
    $env:RUSTFLAGS = "-C link-arg=-Wl,-z,max-page-size=16384"
    cargo ndk -t $Abi -P $minSdk build --release -p neoguitar-android
    if ($LASTEXITCODE -ne 0) { throw "cargo ndk failed" }
    $triple = @{ "arm64-v8a" = "aarch64-linux-android"; "armeabi-v7a" = "armv7-linux-androideabi"; "x86_64" = "x86_64-linux-android" }[$Abi]
    $so = "$repo\target\$triple\release\libneoguitar_android.so"

    Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$out\assets", "$out\lib\$Abi" | Out-Null
    Copy-Item "$repo\soundfonts\GeneralUser-GS.sf2" "$out\assets\"
    Copy-Item "$repo\samples\extra\Demo v5.gp5" "$out\assets\demo.gp5"
    Copy-Item $so "$out\lib\$Abi\"

    # The one Java class (file picker): javac -> d8 -> classes.dex
    New-Item -ItemType Directory -Force "$out\classes" | Out-Null
    & "$env:JAVA_HOME\bin\javac.exe" -encoding UTF-8 --release 8 -Xlint:-options -cp $jar -d "$out\classes" "$PSScriptRoot\java\com\fiambre\neoguitar\NeoActivity.java"
    if ($LASTEXITCODE -ne 0) { throw "javac failed" }
    $cls = Get-ChildItem "$out\classes" -Recurse -Filter *.class | ForEach-Object { $_.FullName }
    & "$bt\d8.bat" --release --min-api $minSdk --lib $jar --output "$out" $cls
    if ($LASTEXITCODE -ne 0) { throw "d8 failed" }

    & "$bt\aapt2.exe" compile --dir "$PSScriptRoot\res" -o "$out\res.zip"
    if ($LASTEXITCODE -ne 0) { throw "aapt2 compile failed" }
    & "$bt\aapt2.exe" link -o "$out\base.apk" -I $jar --manifest "$PSScriptRoot\AndroidManifest.xml" -A "$out\assets" "$out\res.zip" `
        --min-sdk-version $minSdk --target-sdk-version $targetSdk --version-code $VersionCode --version-name $versionName
    if ($LASTEXITCODE -ne 0) { throw "aapt2 failed" }

    # Native libraries go in stored (not compressed) and 16 KB aligned, so they are mapped straight from the APK.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open("$out\base.apk", "Update")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\classes.dex", "classes.dex", "Optimal")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\lib\$Abi\libneoguitar_android.so", "lib/$Abi/libneoguitar_android.so", "NoCompression")
    $zip.Dispose()

    & "$bt\zipalign.exe" -f -P 16 4 "$out\base.apk" "$out\aligned.apk"
    if ($LASTEXITCODE -ne 0) { throw "zipalign failed" }

    $ks = "$env:USERPROFILE\.android\debug.keystore"
    if (-not (Test-Path $ks)) {
        New-Item -ItemType Directory -Force (Split-Path $ks) | Out-Null
        & "$env:JAVA_HOME\bin\keytool.exe" -genkeypair -v -keystore $ks -storepass android -keypass android -alias androiddebugkey `
            -keyalg RSA -keysize 2048 -validity 10000 -dname "CN=Android Debug,O=Android,C=US"
    }
    $apk = "$out\NeoGuitar-debug.apk"
    & "$bt\apksigner.bat" sign --ks $ks --ks-pass pass:android --key-pass pass:android --out $apk "$out\aligned.apk"
    if ($LASTEXITCODE -ne 0) { throw "apksigner failed" }
    Write-Host "APK: $apk ($([math]::Round((Get-Item $apk).Length / 1MB, 1)) MB) versionName=$versionName versionCode=$VersionCode"

    if ($Bundle) {
        if (-not $Keystore -or -not $env:NEOGUITAR_KS_PASS) { throw "-Bundle needs -Keystore and the NEOGUITAR_KS_PASS environment variable" }
        if (-not $bundletool) { throw "bundletool-all-*.jar not found in $env:LOCALAPPDATA\Android\tools" }
        # Same resources again, in protobuf format (what an AAB module holds).
        & "$bt\aapt2.exe" link --proto-format -o "$out\base-proto.apk" -I $jar --manifest "$PSScriptRoot\AndroidManifest.xml" -A "$out\assets" "$out\res.zip" `
            --min-sdk-version $minSdk --target-sdk-version $targetSdk --version-code $VersionCode --version-name $versionName
        if ($LASTEXITCODE -ne 0) { throw "aapt2 (proto) failed" }
        $mod = "$out\module"
        Expand-Archive "$out\base-proto.apk" $mod -Force
        New-Item -ItemType Directory -Force "$mod\manifest", "$mod\dex", "$mod\lib\$Abi" | Out-Null
        Move-Item "$mod\AndroidManifest.xml" "$mod\manifest\AndroidManifest.xml"
        Copy-Item "$out\classes.dex" "$mod\dex\classes.dex"
        Copy-Item $so "$mod\lib\$Abi\"
        $baseZip = "$out\base.zip"
        $z = [IO.Compression.ZipFile]::Open($baseZip, "Create")
        Get-ChildItem $mod -Recurse -File | ForEach-Object {
            $name = $_.FullName.Substring($mod.Length + 1).Replace("\", "/")
            [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($z, $_.FullName, $name, "Optimal")
        }
        $z.Dispose()
        '{"optimizations":{"uncompressNativeLibraries":{"enabled":true,"alignment":"PAGE_ALIGNMENT_16K"}}}' | Set-Content "$out\BundleConfig.json" -Encoding ascii
        $aab = "$out\NeoGuitar-$versionName.aab"
        & "$env:JAVA_HOME\bin\java.exe" -jar $bundletool build-bundle --modules="$baseZip" --config="$out\BundleConfig.json" --output="$aab" --overwrite
        if ($LASTEXITCODE -ne 0) { throw "bundletool failed" }
        & "$env:JAVA_HOME\bin\jarsigner.exe" -keystore $Keystore -storepass:env NEOGUITAR_KS_PASS -sigalg SHA256withRSA -digestalg SHA-256 $aab $Alias
        if ($LASTEXITCODE -ne 0) { throw "jarsigner failed" }
        Write-Host "AAB: $aab ($([math]::Round((Get-Item $aab).Length / 1MB, 1)) MB)"
        # Also a release APK signed with the same key, so manual installs can be updated in place.
        $relApk = "$out\NeoGuitar-$versionName.apk"
        & "$bt\apksigner.bat" sign --ks $Keystore --ks-pass env:NEOGUITAR_KS_PASS --ks-key-alias $Alias --out $relApk "$out\aligned.apk"
        if ($LASTEXITCODE -ne 0) { throw "apksigner (release) failed" }
        Write-Host "APK: $relApk"
    }

    if ($Install) {
        & "$sdk\platform-tools\adb.exe" install -r $apk
        if ($LASTEXITCODE -ne 0) { throw "adb install failed" }
        & "$sdk\platform-tools\adb.exe" shell am start -n com.fiambre.neoguitar/.NeoActivity
    }
} finally { Pop-Location }
