# Builds the NeoGuitar Tabs companion APK without Gradle: cargo-ndk -> javac/d8 -> aapt2 -> zipalign -> apksigner.
# Debug key (default):    build-apk.ps1 [-Install]
# Release key:            build-apk.ps1 -Keystore <file.jks> [-Alias upload]   (password in NEOGUITAR_KS_PASS)
param([switch]$Install, [string]$Abi = "arm64-v8a", [string]$Keystore, [string]$Alias = "upload", [int]$VersionCode = 1)

$ErrorActionPreference = "Stop"
$repo = Resolve-Path "$PSScriptRoot\..\.."
$sdk = if ($env:ANDROID_HOME) { $env:ANDROID_HOME } else { "$env:LOCALAPPDATA\Android\Sdk" }
$env:ANDROID_HOME = $sdk
if (-not $env:ANDROID_NDK_HOME) { $env:ANDROID_NDK_HOME = (Get-ChildItem "$sdk\ndk" -Directory | Sort-Object Name | Select-Object -Last 1).FullName }
if (-not $env:JAVA_HOME) { $env:JAVA_HOME = (Get-ChildItem "C:\Program Files\Microsoft" -Directory -Filter "jdk-17*" | Select-Object -First 1).FullName }
$bt = (Get-ChildItem "$sdk\build-tools" -Directory | Sort-Object Name | Select-Object -Last 1).FullName
$jar = "$sdk\platforms\android-36\android.jar"
$out = "$repo\target\android-tabs"
$minSdk = 26
$targetSdk = 36
$versionName = (Select-String -Path "$repo\neoguitar-tabs-android\Cargo.toml" -Pattern '^version = "(.+)"').Matches[0].Groups[1].Value

Push-Location $repo
try {
    $env:RUSTFLAGS = "-C link-arg=-Wl,-z,max-page-size=16384"   # 16 KB pages (Android 15+)
    cargo ndk -t $Abi -P $minSdk build --release -p neoguitar-tabs-android
    if ($LASTEXITCODE -ne 0) { throw "cargo ndk failed" }
    $triple = @{ "arm64-v8a" = "aarch64-linux-android"; "armeabi-v7a" = "armv7-linux-androideabi"; "x86_64" = "x86_64-linux-android" }[$Abi]
    $so = "$repo\target\$triple\release\libneoguitar_tabs.so"

    Remove-Item $out -Recurse -Force -ErrorAction SilentlyContinue
    New-Item -ItemType Directory -Force "$out\classes", "$out\lib\$Abi" | Out-Null
    Copy-Item $so "$out\lib\$Abi\"

    $src = Get-ChildItem "$PSScriptRoot\java" -Recurse -Filter *.java | ForEach-Object { $_.FullName }
    & "$env:JAVA_HOME\bin\javac.exe" -encoding UTF-8 --release 8 -Xlint:-options -cp $jar -d "$out\classes" $src
    if ($LASTEXITCODE -ne 0) { throw "javac failed" }
    $cls = Get-ChildItem "$out\classes" -Recurse -Filter *.class | ForEach-Object { $_.FullName }
    & "$bt\d8.bat" --release --min-api $minSdk --lib $jar --output "$out" $cls
    if ($LASTEXITCODE -ne 0) { throw "d8 failed" }

    & "$bt\aapt2.exe" compile --dir "$PSScriptRoot\res" -o "$out\res.zip"
    if ($LASTEXITCODE -ne 0) { throw "aapt2 compile failed" }
    & "$bt\aapt2.exe" link -o "$out\base.apk" -I $jar --manifest "$PSScriptRoot\AndroidManifest.xml" "$out\res.zip" `
        --min-sdk-version $minSdk --target-sdk-version $targetSdk --version-code $VersionCode --version-name $versionName
    if ($LASTEXITCODE -ne 0) { throw "aapt2 failed" }

    Add-Type -AssemblyName System.IO.Compression.FileSystem
    $zip = [IO.Compression.ZipFile]::Open("$out\base.apk", "Update")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\classes.dex", "classes.dex", "Optimal")
    [void][IO.Compression.ZipFileExtensions]::CreateEntryFromFile($zip, "$out\lib\$Abi\libneoguitar_tabs.so", "lib/$Abi/libneoguitar_tabs.so", "NoCompression")
    $zip.Dispose()

    & "$bt\zipalign.exe" -f -P 16 4 "$out\base.apk" "$out\aligned.apk"
    if ($LASTEXITCODE -ne 0) { throw "zipalign failed" }

    if ($Keystore) {
        if (-not $env:NEOGUITAR_KS_PASS) { throw "set NEOGUITAR_KS_PASS" }
        $apk = "$out\NeoGuitar-Tabs-$versionName.apk"
        & "$bt\apksigner.bat" sign --ks $Keystore --ks-pass env:NEOGUITAR_KS_PASS --ks-key-alias $Alias --out $apk "$out\aligned.apk"
    } else {
        $ks = "$env:USERPROFILE\.android\debug.keystore"
        $apk = "$out\NeoGuitar-Tabs-debug.apk"
        & "$bt\apksigner.bat" sign --ks $ks --ks-pass pass:android --key-pass pass:android --out $apk "$out\aligned.apk"
    }
    if ($LASTEXITCODE -ne 0) { throw "apksigner failed" }
    Write-Host "APK: $apk ($([math]::Round((Get-Item $apk).Length / 1MB, 1)) MB) versionName=$versionName versionCode=$VersionCode"

    if ($Install) {
        & "$sdk\platform-tools\adb.exe" install -r $apk
        if ($LASTEXITCODE -ne 0) { throw "adb install failed" }
        & "$sdk\platform-tools\adb.exe" shell am start -n com.fiambre.neoguitar.tabs/.TabsActivity
    }
} finally { Pop-Location }
