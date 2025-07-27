//! # ExtProc Load Tester
//!
//! This tool is used to test the load capabilities of an ExtProc server.

#![warn(
    non_ascii_idents,
    rust_2021_prefixes_incompatible_syntax,
    rust_2024_guarded_string_incompatible_syntax,
    unused_crate_dependencies
)]

#[deny(clippy::correctness)]
#[warn(
    clippy::suspicious,
    clippy::complexity,
    clippy::perf,
    clippy::style,
    clippy::pedantic,
    absolute_paths_not_starting_with_crate
)]
#[warn(
    ambiguous_negative_literals,
    closure_returning_async_block,
    deprecated_safe_2024,
    deref_into_dyn_supertrait,
    edition_2024_expr_fragment_specifier,
    elided_lifetimes_in_paths,
    explicit_outlives_requirements,
    ffi_unwind_calls,
    if_let_rescope,
    impl_trait_overcaptures,
    impl_trait_redundant_captures,
    keyword_idents_2018,
    keyword_idents_2024,
    let_underscore_drop,
    macro_use_extern_crate,
    meta_variable_misuse,
    missing_copy_implementations,
    missing_debug_implementations,
    missing_docs,
    missing_unsafe_on_extern,
    redundant_imports,
    redundant_lifetimes,
    rust_2021_incompatible_closure_captures,
    rust_2021_incompatible_or_patterns,
    rust_2021_prelude_collisions,
    rust_2024_incompatible_pat,
    rust_2024_prelude_collisions,
    single_use_lifetimes,
    tail_expr_drop_order,
    trivial_casts,
    trivial_numeric_casts,
    unit_bindings,
    unnameable_types,
    unreachable_pub,
    unsafe_attr_outside_unsafe,
    unsafe_code,
    unsafe_op_in_unsafe_fn,
    unstable_features,
    unused_extern_crates,
    unused_import_braces,
    unused_lifetimes,
    unused_macro_rules,
    unused_qualifications,
    unused_results,
    variant_size_differences
)]
#[allow(clippy::restriction)]
mod app;

mod generated;

#[tokio::main]
async fn main() {
    // NOTE: Spawning a task so that the load test is not scheduled on the main thread, and instead
    // on the worker thread pool.
    let result = tokio::spawn(app::run()).await.expect("Could not join task");
    if let Err(e) = result {
        eprintln!("Error: {e}");
        std::process::exit(e.exit_code());
    }
}
