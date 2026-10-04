# Mesures

Règle du projet : aucun gain annoncé sans mesure publiée ici, avec son protocole.

## Banc

- VM Windows 11 Entreprise (évaluation) 26H2, build 26300 : KVM, 6 cœurs, 12 Go,
  TPM 2.0 (swtpm), Secure Boot actif.
- Installation neuve, aucune application tierce, session ouverte automatiquement.
- Relevé (`measure.ps1`) : nombre de processus, services en cours d'exécution, RAM
  utilisée = totale − disponible (`Win32_OperatingSystem`).
- Protocole : redémarrage, 4 à 5 minutes de repos, puis 3 à 9 relevés espacés de 20 s.

## 1. Mode Jeu / `prism ram clean` — 2026-10-04

Programme inactif de 3 Go (rôle de « Word resté ouvert »), puis `prism ram clean` :

| | Avant | Après |
|---|---|---|
| RAM libre | 5,3 Go (44 %) | 8,3 Go (68 %) |
| Libéré | | **2,9 Go** |
| Cache des fichiers (priorité 5) | | intact |

Détail des mécanismes (liste modifiée, priorité 0) : spec v0.1 §4.1.

## 2. Allègement des services — 2026-10-04

| Configuration | Processus | Services actifs | RAM utilisée (médiane) |
|---|---|---|---|
| Windows d'usine, au calme (après `restore`, 9 relevés) | 113 | 78 | 2 065 Mo |
| Niveau `sûr` (3 relevés) | 113 | 79 | 2 030 Mo |
| Niveaux `sûr` + `avancé` (9 relevés) | 103 | 71 | 1 970 Mo |

Lecture :
- **`sûr` ne change rien de mesurable au repos.** Sur Windows d'usine, seuls 2 de ses
  services tournent (DiagTrack, TrkWks) ; les autres sont déjà arrêtés et ne démarrent
  qu'à la demande. Son intérêt est la vie privée (télémétrie, identifiant publicitaire,
  historique d'activité), pas la performance.
- **`avancé` : −10 processus, −7 services, ≈ −95 Mo (−5 %).** Il arrête 5 services qui
  tournent vraiment : SysMain, Recherche Windows, Spouleur, PcaSvc, CDPSvc.
- `prism allege restore` remet les 24 valeurs d'origine sans échec ; l'état relu
  correspond ligne à ligne à l'état d'usine, démarrages différés compris.

### Erreur corrigée

Une première référence, prise juste après l'installation, donnait 124 processus et
2,66 Go : Windows terminait alors ses tâches de premier démarrage. Comparée à elle, le
niveau `sûr` semblait gagner 11 processus et 620 Mo. C'était faux. La référence
retenue est celle d'un Windows au calme, mesurée après restauration.

### Conclusion

Désactiver des services Windows rapporte peu (≈ 100 Mo, une dizaine de processus).
Les gains importants viennent :
1. du **Mode Jeu** (RAM des programmes inactifs rendue au jeu : 2,9 Go mesurés) ;
2. des **applications lancées au démarrage**, absentes de cette VM neuve mais
   présentes sur tout PC réel (Edge, OneDrive, Teams, lanceurs, utilitaires RGB) :
   prochain chantier, à mesurer de la même façon.

## Ce qui n'est pas mesurable en VM

- **FPS et micro-saccades** : il faut un PC de jeu réel, avec un protocole publié ici.
- **Anti-cheats noyau** (Vanguard, FACEIT) : ils refusent de démarrer en machine
  virtuelle. Leur compatibilité se vérifie sur PC réel. Prism n'y touche pas par
  construction (`anticheat-rules.md`), et aucun service dont ils dépendent n'est dans
  le catalogue (test `anticheat_critical_services_are_protected`).
