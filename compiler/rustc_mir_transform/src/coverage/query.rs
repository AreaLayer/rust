use rustc_data_structures::captures::Captures;
use rustc_middle::middle::codegen_fn_attrs::CodegenFnAttrFlags;
use rustc_middle::mir::coverage::{CounterId, CoverageKind};
use rustc_middle::mir::{Body, CoverageIdsInfo, Statement, StatementKind};
use rustc_middle::query::TyCtxtAt;
use rustc_middle::ty::{self, TyCtxt};
use rustc_middle::util::Providers;
use rustc_span::def_id::LocalDefId;
use rustc_span::sym;
use tracing::trace;

/// Registers query/hook implementations related to coverage.
pub(crate) fn provide(providers: &mut Providers) {
    providers.hooks.is_eligible_for_coverage =
        |TyCtxtAt { tcx, .. }, def_id| is_eligible_for_coverage(tcx, def_id);
    providers.queries.coverage_attr_on = coverage_attr_on;
    providers.queries.coverage_ids_info = coverage_ids_info;
}

/// Hook implementation for [`TyCtxt::is_eligible_for_coverage`].
fn is_eligible_for_coverage(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    // Only instrument functions, methods, and closures (not constants since they are evaluated
    // at compile time by Miri).
    // FIXME(#73156): Handle source code coverage in const eval, but note, if and when const
    // expressions get coverage spans, we will probably have to "carve out" space for const
    // expressions from coverage spans in enclosing MIR's, like we do for closures. (That might
    // be tricky if const expressions have no corresponding statements in the enclosing MIR.
    // Closures are carved out by their initial `Assign` statement.)
    if !tcx.def_kind(def_id).is_fn_like() {
        trace!("InstrumentCoverage skipped for {def_id:?} (not an fn-like)");
        return false;
    }

    // Don't instrument functions with `#[automatically_derived]` on their
    // enclosing impl block, on the assumption that most users won't care about
    // coverage for derived impls.
    if let Some(impl_of) = tcx.impl_of_method(def_id.to_def_id())
        && tcx.is_automatically_derived(impl_of)
    {
        trace!("InstrumentCoverage skipped for {def_id:?} (automatically derived)");
        return false;
    }

    if tcx.codegen_fn_attrs(def_id).flags.contains(CodegenFnAttrFlags::NAKED) {
        trace!("InstrumentCoverage skipped for {def_id:?} (`#[naked]`)");
        return false;
    }

    if !tcx.coverage_attr_on(def_id) {
        trace!("InstrumentCoverage skipped for {def_id:?} (`#[coverage(off)]`)");
        return false;
    }

    true
}

/// Query implementation for `coverage_attr_on`.
fn coverage_attr_on(tcx: TyCtxt<'_>, def_id: LocalDefId) -> bool {
    // Check for annotations directly on this def.
    if let Some(attr) = tcx.get_attr(def_id, sym::coverage) {
        match attr.meta_item_list().as_deref() {
            Some([item]) if item.has_name(sym::off) => return false,
            Some([item]) if item.has_name(sym::on) => return true,
            Some(_) | None => {
                // Other possibilities should have been rejected by `rustc_parse::validate_attr`.
                tcx.dcx().span_bug(attr.span, "unexpected value of coverage attribute");
            }
        }
    }

    match tcx.opt_local_parent(def_id) {
        // Check the parent def (and so on recursively) until we find an
        // enclosing attribute or reach the crate root.
        Some(parent) => tcx.coverage_attr_on(parent),
        // We reached the crate root without seeing a coverage attribute, so
        // allow coverage instrumentation by default.
        None => true,
    }
}

/// Query implementation for `coverage_ids_info`.
fn coverage_ids_info<'tcx>(
    tcx: TyCtxt<'tcx>,
    instance_def: ty::InstanceKind<'tcx>,
) -> CoverageIdsInfo {
    let mir_body = tcx.instance_mir(instance_def);

    let max_counter_id = all_coverage_in_mir_body(mir_body)
        .filter_map(|kind| match *kind {
            CoverageKind::CounterIncrement { id } => Some(id),
            _ => None,
        })
        .max()
        .unwrap_or(CounterId::ZERO);

    CoverageIdsInfo { max_counter_id }
}

fn all_coverage_in_mir_body<'a, 'tcx>(
    body: &'a Body<'tcx>,
) -> impl Iterator<Item = &'a CoverageKind> + Captures<'tcx> {
    body.basic_blocks.iter().flat_map(|bb_data| &bb_data.statements).filter_map(|statement| {
        match statement.kind {
            StatementKind::Coverage(ref kind) if !is_inlined(body, statement) => Some(kind),
            _ => None,
        }
    })
}

fn is_inlined(body: &Body<'_>, statement: &Statement<'_>) -> bool {
    let scope_data = &body.source_scopes[statement.source_info.scope];
    scope_data.inlined.is_some() || scope_data.inlined_parent_scope.is_some()
}
