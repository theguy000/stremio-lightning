const fs = require('fs');
const path = require('path');

const newVersion = process.argv[2];
if (!newVersion) {
  console.error("Usage: node scripts/bump-version.cjs <version>");
  process.exit(1);
}

// Ensure the version format is correct (e.g. X.Y.Z)
if (!/^\d+\.\d+\.\d+(?:-[\w.]+)?$/.test(newVersion)) {
  console.error(`Invalid version format: ${newVersion}. Must be semver like X.Y.Z`);
  process.exit(1);
}
const packageJsonPath = path.join(__dirname, '../package.json');
const packageJson = JSON.parse(fs.readFileSync(packageJsonPath, 'utf8'));
const oldVersion = packageJson.version;

console.log(`Bumping version from ${oldVersion} to ${newVersion}...`);

const metainfoPath = path.join(__dirname, '../assets/io.github.theguy000.stremio-lightning.metainfo.xml');
const metainfo = fs.readFileSync(metainfoPath, 'utf8');
const escapedVersion = newVersion.replace(/[.*+?^${}()|[\]\\]/g, '\\$&');
const releasePattern = new RegExp(`<release\\s+version="${escapedVersion}"\\s+date="\\d{4}-\\d{2}-\\d{2}"[^>]*>`);

if (!releasePattern.test(metainfo)) {
  console.error(`Missing dated AppStream release ${newVersion}. Add it to assets/io.github.theguy000.stremio-lightning.metainfo.xml before tagging.`);
  process.exit(1);
}

if (oldVersion === newVersion) {
  console.log("Package version is already current.");
  process.exit(0);
}

function updateFile(relativePath, replacements) {
  const absolutePath = path.join(__dirname, '..', relativePath);
  if (!fs.existsSync(absolutePath)) {
    console.warn(`Warning: File not found: ${relativePath}`);
    return;
  }
  let content = fs.readFileSync(absolutePath, 'utf8');
  let original = content;

  for (const [target, replacement] of replacements) {
    if (typeof target === 'string') {
      content = content.split(target).join(replacement);
    } else if (target instanceof RegExp) {
      content = content.replace(target, replacement);
    }
  }

  if (content !== original) {
    fs.writeFileSync(absolutePath, content, 'utf8');
    console.log(`Updated ${relativePath}`);
  } else {
    console.log(`No changes for ${relativePath}`);
  }
}

// 1. package.json
updateFile('package.json', [
  [`"version": "${oldVersion}"`, `"version": "${newVersion}"`]
]);

// 2. package-lock.json
updateFile('package-lock.json', [
  [`"version": "${oldVersion}"`, `"version": "${newVersion}"`]
]);

// 3. Cargo.toml files
const cargoTomlFiles = [
  'crates/xtask/Cargo.toml',
  'crates/stremio-lightning-core/Cargo.toml',
  'crates/stremio-lightning-linux/Cargo.toml',
  'crates/stremio-lightning-macos/Cargo.toml',
  'crates/stremio-lightning-windows/Cargo.toml'
];
for (const file of cargoTomlFiles) {
  updateFile(file, [
    [`version = "${oldVersion}"`, `version = "${newVersion}"`]
  ]);
}

// 4. app_update.rs
updateFile('crates/stremio-lightning-core/src/app_update.rs', [
  [`current_version: "${oldVersion}".to_string()`, `current_version: "${newVersion}".to_string()`],
  [`"currentVersion": "${oldVersion}"`, `"currentVersion": "${newVersion}"`]
]);

// 5. host_api.rs
updateFile('crates/stremio-lightning-core/src/host_api.rs', [
  [`handshake_response("${oldVersion}")`, `handshake_response("${newVersion}")`],
  [`"shellVersion", "", "${oldVersion}"`, `"shellVersion", "", "${newVersion}"`]
]);

// 6. windows-shell.exe.manifest
updateFile('crates/stremio-lightning-windows/windows-shell.exe.manifest', [
  [`version="${oldVersion}.0"`, `version="${newVersion}.0"`]
]);

// 7. Info.plist
// Info.plist has two places for version:
// <key>CFBundleShortVersionString</key>
// <string>0.1.0</string>
// <key>CFBundleVersion</key>
// <string>0.1.0</string>
updateFile('crates/stremio-lightning-macos/resources/Info.plist', [
  [new RegExp(`<key>CFBundleShortVersionString</key>\\s*<string>${oldVersion}</string>`), `<key>CFBundleShortVersionString</key>\n  <string>${newVersion}</string>`],
  [new RegExp(`<key>CFBundleVersion</key>\\s*<string>${oldVersion}</string>`), `<key>CFBundleVersion</key>\n  <string>${newVersion}</string>`]
]);

// 8. host_contract.json
updateFile('crates/stremio-lightning-windows/tests/fixtures/host_contract.json', [
  [`"shellVersion": "${oldVersion}"`, `"shellVersion": "${newVersion}"`]
]);

// 9. host.rs (Windows)
updateFile('crates/stremio-lightning-windows/src/host.rs', [
  [`WindowsHost::new("${oldVersion}")`, `WindowsHost::new("${newVersion}")`]
]);

console.log("Version bump completed successfully!");
