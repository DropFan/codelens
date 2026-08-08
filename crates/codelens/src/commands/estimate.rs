//! `codelens estimate` — development cost estimation with pluggable models
//! (COCOMO I/II, Putnam, LOCOMO).

use anyhow::{Context, Result};

use codelens_core::analyze;
use codelens_core::output::Report;

use super::{load_partial_config, resolve_config, write_report};
use crate::cli;

pub(crate) fn run_estimate(args: &cli::EstimateArgs, advanced: &cli::AdvancedArgs) -> Result<()> {
    let partial = load_partial_config(advanced)?;
    let config = resolve_config(&args.filter, &args.output, advanced, partial.as_ref());
    let result = analyze(&args.paths, &config).context("Analysis failed")?;

    let cost_config = codelens_core::CostConfig {
        average_wage: args.avg_wage,
        overhead: args.overhead,
    };

    if matches!(args.model, cli::ModelArg::All) {
        return run_estimate_all(&result.summary, args, &cost_config, &config.output);
    }

    let model: Box<dyn codelens_core::EstimationModel> = build_model(args);

    let report =
        codelens_core::insight::estimation::estimate(&result.summary, model.as_ref(), &cost_config);
    write_report(Report::Estimation(report), &config.output)
}

fn build_model(args: &cli::EstimateArgs) -> Box<dyn codelens_core::EstimationModel> {
    match args.model {
        cli::ModelArg::CocomoBasic | cli::ModelArg::All => {
            Box::new(codelens_core::CocomoBasicModel {
                project_type: args.project_type.into(),
                eaf: args.eaf,
            })
        }
        cli::ModelArg::Cocomo2 => {
            let mut m = codelens_core::CocomoIIModel::default();
            if let Some(sf) = args.sf_sum {
                let per_factor = sf / 5.0;
                m.scale_factors = [per_factor; 5];
            }
            m.eaf = args.eaf;
            Box::new(m)
        }
        cli::ModelArg::Putnam => Box::new(codelens_core::PutnamModel {
            ck: args.ck,
            d0: args.d0,
        }),
        cli::ModelArg::Locomo => Box::new(build_locomo_model(args)),
    }
}

/// Resolve LOCOMO pricing: preset base values (matching scc), overridden
/// by any explicitly passed --llm-* flag.
fn build_locomo_model(args: &cli::EstimateArgs) -> codelens_core::LocomoModel {
    let (base_in, base_out, base_tps) = match args.locomo_preset.unwrap_or_default() {
        cli::LocomoPresetArg::Large => (10.0, 30.0, 30.0),
        cli::LocomoPresetArg::Medium => (3.0, 15.0, 50.0),
        cli::LocomoPresetArg::Small => (0.5, 2.0, 100.0),
        cli::LocomoPresetArg::Local => (0.0, 0.0, 15.0),
    };
    codelens_core::LocomoModel {
        input_price_per_m: args.llm_input_price.unwrap_or(base_in),
        output_price_per_m: args.llm_output_price.unwrap_or(base_out),
        tokens_per_second: args.llm_tps.unwrap_or(base_tps),
        ..Default::default()
    }
}

fn run_estimate_all(
    summary: &codelens_core::Summary,
    args: &cli::EstimateArgs,
    cost_config: &codelens_core::CostConfig,
    output: &codelens_core::config::OutputConfig,
) -> Result<()> {
    let cocomo_basic = codelens_core::CocomoBasicModel {
        project_type: args.project_type.into(),
        eaf: args.eaf,
    };
    let mut cocomo2 = codelens_core::CocomoIIModel::default();
    if let Some(sf) = args.sf_sum {
        cocomo2.scale_factors = [sf / 5.0; 5];
    }
    cocomo2.eaf = args.eaf;
    let putnam = codelens_core::PutnamModel {
        ck: args.ck,
        d0: args.d0,
    };
    let locomo = build_locomo_model(args);

    let models: Vec<&dyn codelens_core::EstimationModel> =
        vec![&cocomo_basic, &cocomo2, &putnam, &locomo];
    let comparison =
        codelens_core::insight::estimation::estimate_all(summary, &models, cost_config);
    write_report(Report::EstimationComparison(comparison), output)
}
