#!/bin/sh
# Crea il certificato autofirmato di code signing "RuneMD Local" in un portachiavi dedicato. Idempotente.
set -eu
NAME="RuneMD Local"
KC="$HOME/Library/Keychains/runemd.keychain-db"
KCPASS=runemd
if [ -f "$KC" ]; then security unlock-keychain -p "$KCPASS" "$KC"; fi
if [ -f "$KC" ] && security find-identity -p codesigning "$KC" | grep -q "\"$NAME\""; then echo "già presente: $NAME"; exit 0; fi
D=$(mktemp -d); trap 'rm -rf "$D"' EXIT
cat > "$D/openssl.cnf" <<CNF
[req]
distinguished_name = dn
x509_extensions = ext
prompt = no
[dn]
CN = $NAME
[ext]
basicConstraints = critical,CA:false
keyUsage = critical,digitalSignature
extendedKeyUsage = critical,codeSigning
CNF
openssl req -x509 -newkey rsa:2048 -nodes -days 3650 -config "$D/openssl.cnf" -keyout "$D/k.pem" -out "$D/c.pem" 2>/dev/null
openssl pkcs12 -export -legacy -inkey "$D/k.pem" -in "$D/c.pem" -out "$D/c.p12" -passout pass:runemd 2>/dev/null \
  || openssl pkcs12 -export -inkey "$D/k.pem" -in "$D/c.pem" -out "$D/c.p12" -passout pass:runemd
# Portachiavi dedicato con password nota (il certificato è locale e senza valore): nel login
# keychain codesign aspetta un dialogo GUI, qui la partition list lo evita.
[ -f "$KC" ] || security create-keychain -p "$KCPASS" "$KC"
security set-keychain-settings "$KC"
security unlock-keychain -p "$KCPASS" "$KC"
security import "$D/c.p12" -k "$KC" -P runemd -T /usr/bin/codesign
security set-key-partition-list -S apple-tool:,apple:,codesign: -s -k "$KCPASS" "$KC" >/dev/null
security list-keychains -d user | grep -q runemd.keychain || security list-keychains -d user -s "$KC" $(security list-keychains -d user | tr -d '"')
# Niente add-trusted-cert: apre un dialogo GUI, e codesign firma anche con un certificato non fidato.
security find-identity -p codesigning "$KC" | grep "\"$NAME\""
