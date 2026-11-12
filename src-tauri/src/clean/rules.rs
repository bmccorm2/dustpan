//! Cache rule table. Paths ported from tw93/Mole (`lib/clean/*.sh`, GPL-3.0).
//! Order matters: specific rules first, so they claim paths before the generic sweeps.

use super::{Cat, Rule};

const fn r(
    id: &'static str,
    category: Cat,
    label: &'static str,
    globs: &'static [&'static str],
    app_bundle: Option<&'static str>,
) -> Rule {
    Rule { id, category, label, globs, recommended: true, app_bundle }
}

const fn dev(id: &'static str, label: &'static str, globs: &'static [&'static str]) -> Rule {
    r(id, Cat::Developer, label, globs, None)
}

pub const RULES: &[Rule] = &[
    // Browsers
    r("safari", Cat::Browsers, "Safari cache", &["Library/Caches/com.apple.Safari/*"], Some("com.apple.Safari")),
    r(
        "chrome",
        Cat::Browsers,
        "Chrome cache",
        &[
            "Library/Application Support/Google/Chrome/*/Cache/*",
            "Library/Application Support/Google/Chrome/*/Code Cache/*",
            "Library/Application Support/Google/Chrome/*/GPUCache/*",
            "Library/Application Support/Google/Chrome/*/Service Worker/CacheStorage/*",
            "Library/Caches/Google/Chrome/*",
        ],
        Some("com.google.Chrome"),
    ),
    r("edge", Cat::Browsers, "Edge cache", &["Library/Caches/com.microsoft.edgemac/*"], Some("com.microsoft.edgemac")),
    r("brave", Cat::Browsers, "Brave cache", &["Library/Caches/BraveSoftware/*"], Some("com.brave.Browser")),
    r(
        "arc",
        Cat::Browsers,
        "Arc cache",
        &[
            "Library/Caches/company.thebrowser.Browser/*",
            "Library/Application Support/Arc/User Data/*/Code Cache/*",
            "Library/Application Support/Arc/User Data/*/GPUCache/*",
            "Library/Application Support/Arc/User Data/*/DawnCache/*",
            "Library/Application Support/Arc/User Data/*/GrShaderCache/*",
        ],
        Some("company.thebrowser.Browser"),
    ),
    r("firefox", Cat::Browsers, "Firefox cache", &["Library/Caches/Firefox/Profiles/*/cache2/*"], Some("org.mozilla.firefox")),
    // Developer tools
    dev("npm", "npm cache", &[".npm/_cacache/*"]),
    dev("tnpm", "tnpm cache", &[".tnpm/_cacache/*"]),
    dev("yarn", "Yarn cache", &["Library/Caches/Yarn/*"]),
    dev("bun", "Bun cache", &[".bun/install/cache/*"]),
    dev("uv", "uv cache", &[".cache/uv/*"]),
    dev("poetry", "Poetry cache", &[".cache/poetry/*", "Library/Caches/pypoetry/artifacts/*", "Library/Caches/pypoetry/cache/*"]),
    dev("pip", "pip cache", &["Library/Caches/pip/*"]),
    dev("mypy", "MyPy cache", &[".cache/mypy/*"]),
    dev("ruff", "Ruff cache", &[".cache/ruff/*"]),
    dev("precommit", "pre-commit cache", &[".cache/pre-commit/*"]),
    dev("pyenv", "pyenv cache", &[".pyenv/cache/*"]),
    dev("cargo", "Cargo registry cache", &[".cargo/registry/cache/*"]),
    dev("gradle", "Gradle cache", &[".gradle/caches/*"]),
    dev("homebrew", "Homebrew downloads", &["Library/Caches/Homebrew/downloads/*"]),
    dev("deriveddata", "Xcode DerivedData", &["Library/Developer/Xcode/DerivedData/*"]),
    dev("coresim", "CoreSimulator caches", &["Library/Developer/CoreSimulator/Caches/*"]),
    dev("swiftpm", "Swift Package Manager cache", &[".cache/swift-package-manager/*", "Library/Caches/org.swift.swiftpm/*"]),
    dev("cocoapods", "CocoaPods cache", &["Library/Caches/CocoaPods/*"]),
    dev("gobuild", "Go build cache", &["Library/Caches/go-build/*"]),
    dev("nodegyp", "node-gyp cache", &[".cache/node-gyp/*"]),
    dev("typescript", "TypeScript cache", &[".cache/typescript/*"]),
    dev("electron", "Electron cache", &[".cache/electron/*"]),
    dev("prisma", "Prisma cache", &[".cache/prisma/*"]),
    dev("turbo", "Turbo cache", &[".turbo/cache/*"]),
    dev("vite", "Vite cache", &[".vite/cache/*", ".cache/vite/*"]),
    dev("webpack", "Webpack cache", &[".cache/webpack/*"]),
    dev("eslint", "ESLint cache", &[".cache/eslint/*"]),
    dev("prettier", "Prettier cache", &[".cache/prettier/*"]),
    dev("rbenv", "rbenv cache", &[".rbenv/cache/*"]),
    dev("gemspecs", "gem specs cache", &[".gem/specs/*"]),
    dev("composer", "Composer cache", &["Library/Caches/composer/*"]),
    dev("flutter", "Flutter cache", &[".cache/flutter/*"]),
    dev("android", "Android build cache", &[".android/build-cache/*", ".android/cache/*"]),
    dev("bazel", "Bazel cache", &[".cache/bazel/*"]),
    dev("zig", "Zig cache", &[".cache/zig/*"]),
    dev("buildx", "Docker buildx cache", &[".docker/buildx/cache/*"]),
    dev("kube", "kube cache", &[".kube/cache/*"]),
    dev("awscli", "AWS CLI cache", &[".aws/cli/cache/*"]),
    dev("jetbrainslogs", "JetBrains logs", &["Library/Logs/JetBrains/*"]),
    // AI tools
    r(
        "claude",
        Cat::Ai,
        "Claude desktop cache",
        &["Library/Caches/com.anthropic.claudefordesktop/*"],
        Some("com.anthropic.claudefordesktop"),
    ),
    r("claudelogs", Cat::Ai, "Claude logs", &["Library/Logs/Claude/*"], None),
    r("lmstudio", Cat::Ai, "LM Studio cache", &["Library/Caches/com.lmstudio.lmstudio/*"], None),
    r(
        "codex",
        Cat::Ai,
        "Codex app cache",
        &["Library/Caches/Codex/Default/Cache/*", "Library/Caches/Codex/Default/Code Cache/*"],
        None,
    ),
    r("opencode", Cat::Ai, "OpenCode cache", &[".cache/opencode/*"], None),
    r("copilot", Cat::Ai, "Copilot cache", &["Library/Caches/copilot/*"], None),
    // Logs
    r("diagreports", Cat::Logs, "Diagnostic reports", &["Library/Logs/DiagnosticReports/*"], None),
    r("userlogs", Cat::Logs, "User logs", &["Library/Logs/*"], None),
];

/// Children of ~/Library/Caches skipped by the generic sweep (plus our own icon cache).
pub const SWEEP_SKIP: &[&str] = &["CloudKit", "FamilyCircle", "GeoServices", "com.bryan.molesweep"];
