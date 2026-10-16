//! Physiology as pure functions (TRD §5.5–5.6). Both Individual and Population
//! modes call these, so the two modes share one model of the body.

use crate::traits::{Phenotype, Trait};
use serde::{Deserialize, Serialize};

pub const BODY_TEMP: f32 = 38.0;
/// Latent heat of evaporation, J/g.
pub const EVAP_J_PER_G: f32 = 2430.0;
/// Air-temperature reduction from resting in shade, °C.
pub const SHADE_COOLING: f32 = 5.0;
/// Air-temperature gain from sheltering (burrow, lee side, huddling) when resting in cold, °C.
pub const SHELTER_WARMING: f32 = 4.0;
/// Energy content of new tissue, kJ/kg (≈ 7 kJ/g wet mass).
pub const TISSUE_KJ_PER_KG: f32 = 7000.0;

/// Quantities derived from the phenotype and current body mass.
#[derive(Clone, Copy, Debug, Default, Serialize, Deserialize)]
pub struct Derived {
    /// Resting metabolic rate including trait maintenance costs, W.
    pub bmr_w: f32,
    /// Minimum and maximum whole-body thermal conductance, W/°C.
    pub c_min: f32,
    pub c_max: f32,
    /// Maximum evaporative heat loss, W.
    pub evap_cap_w: f32,
    pub vmax_kmh: f32,
    pub cruise_kmh: f32,
    /// How long a full-speed chase can be sustained, s.
    pub stamina_s: f32,
    pub gestation_days: f32,
    pub weaning_days: f32,
    pub maturity_days: f32,
    pub neonate_kg: f32,
    /// Gompertz parameters: hazard(age) = b · exp(g · age_years), per year.
    pub gompertz_b: f32,
    pub gompertz_g: f32,
    /// Energy reserve capacity (fat), kJ.
    pub fat_cap_kj: f32,
    /// Water deficit that kills, g.
    pub water_cap_g: f32,
    /// Basal water loss, g/day.
    pub water_loss_g_day: f32,
    /// Maximum daily food energy the gut can process, kJ/day.
    pub max_intake_kj_day: f32,
    pub plant_eff: f32,
    pub meat_eff: f32,
    /// Perception radius, km.
    pub perception_km: f32,
}

/// Multiplier on BMR from the maintenance cost of expensive traits (no free lunch, TRD TR-B4).
pub fn maintenance_multiplier(p: &Phenotype) -> f32 {
    1.0 + 0.10 * p.get(Trait::Insulation)
        + 0.045 * (p.get(Trait::Brain) - 1.0).max(-0.8)
        + 0.25 * p.get(Trait::Maintenance)
        + 0.08 * p.get(Trait::Ornament)
        + 0.05 * p.get(Trait::Senses)
        + 0.05 * p.get(Trait::Sprint)
        + 0.04 * p.get(Trait::Kidney)
        + 0.04 * p.get(Trait::HeatTolerance)
}

/// Kleiber (1932): BMR = 70 M^0.75 kcal/day → W.
#[inline]
pub fn kleiber_w(mass_kg: f32) -> f32 {
    70.0 * mass_kg.max(1e-3).powf(0.75) * 4184.0 / 86400.0
}

/// Hump-shaped maximum running speed, km/h (Hirt et al. 2017).
#[inline]
pub fn hirt_vmax_kmh(mass_kg: f32) -> f32 {
    let m = mass_kg.max(1e-3);
    25.5 * m.powf(0.26) * (1.0 - (-22.0 * m.powf(-0.6)).exp())
}

/// Net energy cost of moving `distance_m`, kJ (Taylor et al. 1982: 10.7·M^-0.316 J/kg/m).
#[inline]
pub fn locomotion_kj(mass_kg: f32, distance_m: f32) -> f32 {
    10.7 * mass_kg.max(1e-3).powf(0.684) * distance_m / 1000.0
}

pub fn derive(p: &Phenotype, mass_kg: f32) -> Derived {
    let adult = p.get(Trait::BodyMass);
    let m = mass_kg.max(0.005);
    let ins = p.get(Trait::Insulation);
    let app = p.get(Trait::Appendage);
    let sprint = p.get(Trait::Sprint);
    let pace = p.get(Trait::Pace);
    let diet = p.get(Trait::Diet);
    let litter = p.get(Trait::LitterSize);
    let bmr_w = kleiber_w(m) * maintenance_multiplier(p);
    // Thermal conductance scales ~M^0.5; insulation lowers it, appendages raise it (Allen's rule).
    let c_min = 0.27 * m.powf(0.5) / (0.5 + 2.0 * ins) * app.powf(0.3);
    let c_max = c_min * (2.5 + 3.5 * p.get(Trait::HeatTolerance)) * app.powf(0.7);
    let vmax = hirt_vmax_kmh(m) * (0.75 + 0.5 * sprint) * p.get(Trait::LegLength).powf(0.5);
    let gestation = 30.0 * adult.powf(0.25) * pace.powf(0.6);
    Derived {
        bmr_w,
        c_min,
        c_max,
        evap_cap_w: (0.5 + 2.5 * p.get(Trait::HeatTolerance)) * kleiber_w(m),
        vmax_kmh: vmax,
        cruise_kmh: 0.12 * vmax,
        stamina_s: 40.0 * (1.0 + 3.0 * (1.0 - sprint)) * m.powf(0.1),
        gestation_days: gestation,
        weaning_days: 0.8 * gestation * pace.powf(0.7),
        maturity_days: 180.0 * adult.powf(0.25) * pace,
        neonate_kg: 0.07 * adult / litter.powf(0.5),
        gompertz_b: 0.02 * (1.6 - p.get(Trait::Maintenance)),
        gompertz_g: 0.9 * adult.powf(-0.2) / pace,
        fat_cap_kj: 0.18 * m * 1000.0 * 39.0 * 0.8,
        water_cap_g: 0.12 * m * 1000.0,
        water_loss_g_day: 100.0 * m.powf(0.8) * (1.3 - 0.6 * p.get(Trait::Kidney)),
        max_intake_kj_day: 4.5 * kleiber_w(m) * 86.4,
        plant_eff: 0.62 * (1.0 - diet).powf(0.6),
        meat_eff: 0.85 * diet.powf(0.6),
        perception_km: (0.5 + 4.0 * p.get(Trait::Senses)) * m.powf(0.15),
    }
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Thermal {
    /// Extra metabolic heat needed to stay warm, W.
    pub extra_w: f32,
    /// Evaporative cooling needed, W.
    pub evap_w: f32,
    /// > 0 when cooling capacity is exceeded (heat stress).
    pub heat_stress: f32,
    /// > 0 when required heat exceeds ~3× resting metabolism (hypothermia risk).
    pub cold_stress: f32,
}

/// Heat balance at air temperature `ta` with metabolic heat production `heat_w` (TRD TR-PH3).
pub fn thermal(d: &Derived, heat_w: f32, ta: f32) -> Thermal {
    let grad = BODY_TEMP - ta;
    let mut t = Thermal::default();
    if grad > 0.0 {
        let need_c = heat_w / grad;
        if need_c < d.c_min {
            t.extra_w = d.c_min * grad - heat_w;
            t.cold_stress = (t.extra_w / (3.0 * d.bmr_w) - 1.0).max(0.0);
        } else if need_c > d.c_max {
            t.evap_w = heat_w - d.c_max * grad;
        }
    } else {
        t.evap_w = heat_w + d.c_max * (-grad);
    }
    t.heat_stress = (t.evap_w / d.evap_cap_w.max(1e-6) - 1.0).max(0.0);
    t
}

/// Intrinsic (senescent) mortality hazard per year at a given age (Gompertz–Makeham).
#[inline]
pub fn gompertz_hazard(d: &Derived, age_years: f32, background: f32) -> f32 {
    background + d.gompertz_b * (d.gompertz_g * age_years).min(12.0).exp()
}

/// West et al. (2001) ontogenetic growth curve: mass at developmental age `t_days`
/// given adult mass `m_adult`, birth mass `m0`, reaching 90% of adult mass at maturity.
pub fn growth_mass(m_adult: f32, m0: f32, t_days: f32, maturity_days: f32) -> f32 {
    let r0 = (m0 / m_adult).clamp(1e-4, 0.99).powf(0.25);
    let k = ((1.0 - r0) / (1.0 - 0.9f32.powf(0.25))).ln().max(0.1);
    let tau = maturity_days.max(1.0) / k;
    let r = 1.0 - (1.0 - r0) * (-t_days / tau).exp();
    m_adult * r.powi(4)
}

/// Fraction of a plant-eater's digestive efficiency retained on coarse forage.
/// Larger guts ferment fibre better (Jarman–Bell principle; Demment & Van Soest 1985).
#[inline]
pub fn roughage_quality(mass_kg: f32) -> f32 {
    (0.2 * mass_kg.max(1e-3).ln() - 0.25).clamp(0.0, 0.9)
}

/// Camouflage in [0,1]: 1 when coat luminance matches the background.
#[inline]
pub fn camouflage(coat: f32, background: f32) -> f32 {
    (1.0 - 2.5 * (coat - background).abs()).clamp(0.0, 1.0)
}

/// Relative probability a female of preference `pref` and choosiness `choosy` accepts
/// a male with ornament `orn`.
#[inline]
pub fn mate_acceptance(pref: f32, choosy: f32, orn: f32) -> f32 {
    (-choosy * 14.0 * (orn - pref) * (orn - pref)).exp()
}

/// Circular distance between two times of year expressed as fractions.
#[inline]
pub fn year_distance(a: f32, b: f32) -> f32 {
    let d = (a - b).abs() % 1.0;
    d.min(1.0 - d)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::traits::N_TRAITS;

    fn pheno(mass: f32, ins: f32) -> Phenotype {
        let mut v = [0.5f32; N_TRAITS];
        v[Trait::BodyMass as usize] = mass;
        v[Trait::Insulation as usize] = ins;
        v[Trait::Appendage as usize] = 1.0;
        v[Trait::LegLength as usize] = 1.0;
        v[Trait::Brain as usize] = 1.0;
        v[Trait::LitterSize as usize] = 2.0;
        v[Trait::Pace as usize] = 1.0;
        Phenotype(v)
    }

    #[test]
    fn kleiber_matches_reference() {
        // 70 kcal/day for 1 kg ≈ 3.39 W.
        assert!((kleiber_w(1.0) - 3.39).abs() < 0.01);
    }

    #[test]
    fn bigger_and_better_insulated_bodies_lose_less_heat_per_kg() {
        let small = derive(&pheno(2.0, 0.4), 2.0);
        let big = derive(&pheno(20.0, 0.4), 20.0);
        let ts = thermal(&small, small.bmr_w, -10.0);
        let tb = thermal(&big, big.bmr_w, -10.0);
        assert!(ts.extra_w / 2.0 > tb.extra_w / 20.0);
        let thick = derive(&pheno(2.0, 0.9), 2.0);
        assert!(thermal(&thick, thick.bmr_w, -10.0).extra_w < ts.extra_w);
    }

    #[test]
    fn growth_curve_reaches_ninety_percent_at_maturity() {
        let m = growth_mass(50.0, 3.0, 400.0, 400.0);
        assert!((m / 50.0 - 0.9).abs() < 0.01, "{m}");
        assert!(growth_mass(50.0, 3.0, 0.0, 400.0) - 3.0 < 0.01);
    }

    #[test]
    fn speed_peaks_at_intermediate_mass() {
        assert!(hirt_vmax_kmh(60.0) > hirt_vmax_kmh(2.0));
        assert!(hirt_vmax_kmh(60.0) > hirt_vmax_kmh(5000.0));
    }
}
