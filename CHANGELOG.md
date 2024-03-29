# Changelog
All notable changes to this project will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]
## [0.10.1] - 2024-01-01
### Changed
- Implemented configurable max POST body size for media upload

## [0.10.0] - 2023-01-02
### Added
- Read crate version from cargo at build time for providing to templates
- [Breaking] Convert constants + environment variable configuration into toml config

## [0.9.0] - 2023-01-01
### Added
- Adds `MICROPUB_RS_BLOBJECT_STORE_BASE_URI` environment variable to configure the rustyblobjectstore endpoint

### Changed
- Updated deps to latest versions

## [0.8.0] - 2022-12-29
### Changed
- Ported web server to axum from warp
- Nix docker build now tags image based on Cargo.toml

## [0.7.0] - 2022-02-22
### Added
- Strip EXIF tags from images when upload format is identifiable (ImageMagick dependency)
- Nix config for building the project and building a Docker image (improved management of ImageMagick dep)

## [0.6.0] - 2021-09-12
### Added
- Implement micropub photo property.

### Fixed
- Add migration for create media table.

### Changed
- Use pre-made Rust CI Action.
- Use rust 1.55 as build toolchain in Dockerfile.

## [0.5.0] - 2021-09-10
### Added
- Support media upload and fetching.
- Support micropub config query.

### Fixed
- Allow HEAD requests on index handler.

### Changed
- Add ca-certificates package.
- Schema file order shuffle.
- Use build image to reduce published image size.
- Remove template checkout from docker build.
- Release 0.4.0.

## [0.4.0] - 2021-01-11
### Added
- Add bookmark-of support for bookmark style posts.
- Add access logging filter.

### Fixed
- Replace any println calls with log calls.
- Enable multithreaded tokio runtime.

### Changed
- Remote extra newline.

## [0.3.2] - 2021-01-02
### Fixed
- Ensure returned posts are reverse sorted.

### Changed
- Make Dockerfile use rustc 1.49.
- Avoid n+1 tag lookup in atom and archive handlers.
- Clean up DBError handling.
- Rustfmt handler.rs.
- Simplify db connection pool interactions.
- Upgrade Warp dependency.
- Cargo update to upgrade deps in lockfile.
- Remove itertools dep.

## [0.3.1] - 2021-01-01
### Fixed
- Default datetime offset of -8 rather than +7.

### Changed
- Extract datetime parsing into post_util.

## [0.3.0] - 2021-01-01
### Added
- Store new post input bytes into original_blobs table.

### Changed
- Add Dockerfile.

## [0.2.0] - 2020-12-30

Minimum viable version of the `server` and `import_entry` binaries. Just enough
to import from a Pelican static site blog.

### Added
- Support mp-slug property to override default slug.
- Support 'published' datetime micropub property.
- Create initial import_entry binary for importing posts.

### Fixed
- Bind to 0.0.0.0 to enable publically exposing the server.
- Make atom feed post links relative to site root.
- Incorrect published property error log messages.
- Use last (most recent) post rather than first for Atom updated tag.
- Include Atom XML namespace and XML header.

### Changed
- V0.2.0.
- Comments for the archive handler.
- Break up MicropubHandler::verify_auth.
- Extract new_dbconn_pool from server binary.
- Fix warning to use `dyn Fn` in boxed Fn.
- Rename main binary to server.
- Render markdown content_type entries as HTML.
- Add support for parsing micropub JSON with content type as markdown.
- Ensure the MicropubForm's content_type is inserted into DB.
- Wire up micropub format parsing to handle setting content type.
- Put content_type field on various models.
- Add content_type column to posts table.
- Clean up some unused imports.
- Add support for browsing tag archives.
- Add ALL_COLUMNS const tuple for use in select statements.
- Handle micropublish.net style html content in form encoded post creation.
- Fix fetch_post for posts with slashes in slug.
- Initial Atom Feed support.
- Integrate date/time into slugs.
- Abstract away templating and factor out common base context settings.
- Add archive menu item.
- Survey types of posts and properties that still need support.
- Support name and category for JSON entries.
- Support content in quill.p3k.io's JSON format for entries/posts.
- Fix test cases.
- Add props fallbacks in json parser.
- Fix build :facepalm:.
- Remove unneeded default case content type handling.
- First-pass JSON content type micropub handler.
- Rename view_models.Post.categories to Post.tags.
- Include Location header on successful creation.
- Handle single category field or none at all.
- Return 201 Created on new post creation.
- Adds time as a property of our Date view model object.
- Add support for micropub and indieauth in templates.
- Support root of the site displaying latest post.
- Cargo fmt.
- Add ArchiveHandler and support for a /archives page.
- Move FetchHandler and MicropubHandler into handlers submodule.
- Use template env var for reading in templates.
- Static file handler for template assets and template dir env var.
- Hacking on adding template rendering.
- Add query for post categories (tags).
- Extract Posts by_slug query into models.
- Extract Diesel model structs into models module.
- Implement post fetch handler v1.
- Refactor sqlite connection pool creation.
- Write posts into sqlite database.
- Create v1 of posts schema.
- Add diesel as a dep.
- Add .env file to gitignore.
- Add helper for generating URL slugs.
- Parse micropub POST body into a struct.
- Remove commented out code.
- Cargo fmt.
- Check that the token used is for correct 'me'.
- Clean up imports.
- Initial commit.

[Unreleased]: git@github.com:davidwilemski/micropub-rs/compare/0.5.1...HEAD
[0.5.1]: git@github.com:/davidwilemski/micropub-rs/compare/0.5.0...0.5.1
[0.5.0]: git@github.com:/davidwilemski/micropub-rs/compare/0.4.0...0.5.0
[0.4.0]: git@github.com:/davidwilemski/micropub-rs/compare/0.3.2...0.4.0
[0.3.2]: git@github.com:/davidwilemski/micropub-rs/compare/0.3.1...0.3.2
[0.3.1]: git@github.com:/davidwilemski/micropub-rs/compare/0.3.0...0.3.1
[0.3.0]: git@github.com:davidwilemski/micropub-rs/compare/0.2.0...0.3.0
[0.2.0]: git@github.com:davidwilemski/micropub-rs/releases/tag/0.2.0
