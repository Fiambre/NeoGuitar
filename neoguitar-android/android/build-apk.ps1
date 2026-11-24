# Builds the debug-signed APK without Gradle: cargo-ndk -> aapt2 -> zipalign -> apksigner.
# Usage (from the repository root or anywhere): pwsh neoguitar-android\android\build-apk.ps1 [-Install]
param([switch]$Install, [string]$Abi = "arm64-v8a")

$ErrorActionPreference = "Stop"
$repo = Resolve-Path "$PSScriptRoot\..\.."
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$env:ANDROID_HOME = $sdk
if (-not $env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME = (Get-ChildItem "$sdk\ndk" -Directory | Sort-Object Name | Select-Object -Last 1).FullName }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = (Get-ChildItem "C:\Program Files\Microsoft" -Directory -Filter "jdk-17*" | Select-Object -First 1).FullName }
$bt = (Get-ChildItem "$sdk\build-tools" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$jar = "$sdk\platforms\android-34\android.jar"
$out = "$repo\target\android"
$minSdk = 26   # AAudio (low-latency audio) needs API 26

Push-Location $repo
try {
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
    & "$env:JAVA_HOME\bin\javac.exe" --release 8 -Xlint:-options -cp $jar -d "$out\classes" "$PSScriptRoot\java\com\fiambre\neoguitar\NeoActivity.java"
    if ($LASTEXITCODE -ne 0) { throw "javac failed" }
    $cls = Get-ChildItem "$out\classes" -Recurse -Filter *.class | ForEach-Object { $_.FullName }
    & "$bt\d8.bat" --release --min-api $minSdk --lib $jar --output "$out" $cls
    if ($LASTEXITCODE -ne 0) { throw "d8 failed" }

    & "$bt\aapt2.exe" compile --dir "$PSScriptRoot\res" -o "$out\res.zip"
    if ($LASTEXITCODE -ne 0) { throw "aapt2 compile failed" }
    & "$bt\aapt2.exe" link -o "$out\base.apk" -I $jar --manifest "$PSScriptRoot\AndroidManifest.xml" -A "$out\assets" "$out\res.zip" `
        --min-sdk-version $minSdk --target-sdk-version 34 --version-code 1 --version-name 0.1.0
    if ($LASTEXITCODE -ne 0) { throw "aapt2 failed" }

    # Native libraries go into the APK as-is.
    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open("$out\base.apk", "Update")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\classes.dex", "classes.dex", "Optimal")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\lib\$Abi\libneoguitar_android.so", "lib/$Abi/libneoguitar_android.so", "Optimal")
    $zip.Dispose()

    & "$bt\zipalign.exe" -f -p 4 "$out\base.apk" "$out\aligned.apk"
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
    Write-Host "APK: $apk ($([math]::Round((Get-Item $apk).Length / 1MB, 1)) MB)"

    if ($Install) {
        & "$sdk\platform-tools\adb.exe" install -r $apk
        if ($LASTEXITCODE -ne 0) { throw "adb install failed" }
        & "$sdk\platform-tools\adb.exe" shell am start -n com.fiambre.neoguitar/.NeoActivity
    }
} finally { Pop-Location }