//! Calculs des mesures de la barre, comme le Gestionnaire des tâches.

use std::collections::HashMap;

/// Charge de la carte graphique à partir du compteur `\GPU Engine(*)\Utilization
/// Percentage` : une instance par (processus, moteur), nommée
/// `pid_1234_luid_0x…_0x…_phys_0_eng_3_engtype_3D`. Comme le Gestionnaire des tâches :
/// on additionne les processus d'un même moteur, puis on garde le moteur le plus chargé
/// (toutes cartes et tous types confondus : 3D, vidéo, calcul, copie). Additionner tous
/// les moteurs 3D mélangeait la carte intégrée et la carte dédiée.
pub fn gpu_busiest(instances: &[(String, f64)]) -> f64 {
    let mut per_engine: HashMap<&str, f64> = HashMap::new();
    for (name, v) in instances {
        let engine = name.find("luid_").map(|i| &name[i..]).unwrap_or(name.as_str());
        *per_engine.entry(engine).or_default() += v.max(0.0);
    }
    per_engine.values().copied().fold(0.0, f64::max).clamp(0.0, 100.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn gpu_load_is_the_busiest_engine_not_the_sum() {
        let i = |n: &str, v: f64| (n.to_string(), v);
        let samples = vec![
            // Carte dédiée : un jeu (60 %) et le bureau (5 %) sur le même moteur 3D.
            i("pid_100_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 60.0),
            i("pid_200_luid_0x0_0xA_phys_0_eng_0_engtype_3D", 5.0),
            // Même carte, moteur de décodage vidéo.
            i("pid_300_luid_0x0_0xA_phys_0_eng_5_engtype_VideoDecode", 20.0),
            // Carte intégrée : 40 % sur son propre moteur 3D.
            i("pid_400_luid_0x0_0xB_phys_0_eng_0_engtype_3D", 40.0),
        ];
        assert_eq!(gpu_busiest(&samples), 65.0);
        assert_eq!(gpu_busiest(&[]), 0.0);
        assert_eq!(gpu_busiest(&[i("pid_1_luid_x_eng_0_engtype_3D", 250.0)]), 100.0);
    }
}
