## Liste des Tests

### Tests Unitaires
   **Nom du Test** | **Description** | **Catégorie** |
 |----------------|----------------|---------------|
 | `test_build_tree_empty_directory` | Vérifie qu'un répertoire vide est correctement traité en mode arborescent | Structure de base |
 | `test_build_tree_single_file` | Vérifie qu'un seul fichier dans un répertoire est correctement traité | Structure de base |
 | `test_build_tree_nested_structure` | Vérifie une structure imbriquée (répertoire → sous-répertoire → fichier) | Structure imbriquée |
 | `test_build_tree_multiple_files` | Vérifie que plusieurs fichiers sont triés alphabétiquement | Tri alphabétique |
 | `test_build_flat_tree_empty_directory` | Vérifie qu'un répertoire vide est correctement traité en mode plat | Structure de base |
 | `test_build_flat_tree_single_file` | Vérifie qu'un seul fichier est correctement traité en mode plat | Structure de base |
 | `test_build_flat_tree_nested_structure` | Vérifie une structure imbriquée en mode plat avec les références parent | Structure imbriquée |
 | `test_build_flat_tree_excludes_target` | Vérifie que le dossier target est exclu en mode plat | Exclusion |
 | `test_build_tree_excludes_target` | Vérifie que le dossier target est exclu en mode arborescent | Exclusion |
 | `test_tree_and_flat_consistency` | Vérifie que les deux modes (tree et flat) retournent les mêmes chemins | Cohérence |
 | `test_regression_flat_structure` | Test de régression : Vérifie la structure plate sur tests/test_data | Régression |
 | `test_regression_tree_structure` | Test de régression : Vérifie la structure arborescente sur tests/test_data | Régression |
 | `test_regression_alphabetical_order` | Test de régression : Vérifie que les fichiers sont triés alphabétiquement | Régression |
 | `test_regression_flat_alphabetical_order` | Test de régression : Vérifie que les chemins sont triés alphabétiquement en mode plat | Régression |

---

### Tests d'Intégration
 | **Nom du Test** | **Description** | **Catégorie** |
 |----------------|----------------|---------------|
 | `test_integration_tree_output_format` | Vérifie que la sortie JSON en mode arborescent a le bon format | Format JSON |
 | `test_integration_flat_output_format` | Vérifie que la sortie JSON en mode plat a le bon format | Format JSON |
 | `test_integration_complex_structure` | Teste une structure complexe (README.md, src/main.rs, src/lib/utils.rs) | Structure complexe |
 | `test_integration_target_directory_excluded` | Vérifie que le dossier target et son contenu sont exclus dans les deux modes | Exclusion |
 | `test_integration_special_characters_in_filenames` | Vérifie que les fichiers avec des espaces dans leur nom sont correctement traités | Cas spéciaux |
 | `test_integration_deeply_nested_structure` | Teste une structure profondément imbriquée (a/b/c/d/deep.txt) | Structure imbriquée |
 | `test_integration_empty_file` | Vérifie qu'un fichier vide est correctement traité | Cas spéciaux |
 | `test_integration_symlink_handling` | Vérifie que les liens symboliques sont correctement traités (Unix uniquement) | Cas spéciaux |
 | `test_integration_consistency_between_modes` | Vérifie que les deux modes retournent exactement les mêmes chemins | Cohérence |
 | `test_integration_test_data_directory` | Teste le programme sur le dossier tests/test_data | Régression |
 | `test_integration_large_directory_structure` | Teste une structure avec 10 répertoires et 5 fichiers chacun (61 éléments au total) | Performance |

---
---
## Résumé

- **Tests unitaires**: 14
- **Tests d'intégration**: 11
- **Total**: 25 tests

---
---
## Exécution des tests

```bash
# Tous les tests
cargo test

# Seulement les tests unitaires
cargo test --lib

# Seulement les tests d'intégration
cargo test --test integration_tests