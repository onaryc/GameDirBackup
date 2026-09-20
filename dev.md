# Exemples d'utilisation

## Écrire la sortie dans un fichier avec mode arborescent

cargo run -- --output output.json src

cargo run -- -o output.json src

## Écrire la sortie dans un fichier avec mode plat

cargo run -- --output output.json --flat src

cargo run -- -o output.json -f src

## Utilisation normale (sortie sur stdout)

cargo run -- src