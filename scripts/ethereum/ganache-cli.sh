#!/bin/sh
# We use shell (`/bin/sh`) since the Docker image doesn't contain `/bin/bash`

# runs in Docker, so leave default port and export it instead of setting it up here

# Address 0: 0xDf08F82De32B8d460adbE8D72043E3a7e25A3B39 keystore password: address0
# Address 1: 0x5a04A8fB90242fB7E1db7d1F51e268A03b7f93A5 keystore password: address1
# Address 2: 0xe3896ebd3F32092AFC7D27e9ef7b67E26C49fB02 keystore password: address2
# Address 3: 0x0E45891a570Af9e5A962F181C219468A6C9EB4e1 keystore password: address3
# Address 4: 0x8c4B95383a46D30F056aCe085D8f453fCF4Ed66d keystore password: address4
# Address 5: 0x1059B025E3F8b8f76A8120D6D6Fd9fBa172c80b8 keystore password: address5
node /app/ganache-core.docker.cli.js --gasLimit 0xfffffffffffff \
  --db="./snapshot" \
  --deterministic \
  --mnemonic="diary west sketch curious expose decade symptom height minor layer carry man" \
  --account="0x2bdd21761a483f71054e14f5b827213567971c676928d9a1808cbfa4b7501200,9000000000000000000000000000" \
  --account="0xd66ecc35fe42ae2063888bfd11c4573db08bdbd5c2b9b835deef05beb43b407f,9000000000000000000000000000" \
  --account="0x1d00a65debb6143cebc3b48b11db2ddfa81411c1f7c443f706d4f2bd8145ee4a,9000000000000000000000000000" \
  --account="0x30e87297872f658845b755fe9e5ff319605a2513177a871110e25aebfe115f0f,9000000000000000000000000000" \
  --account="0xe001626993056febeae724da4cb5c1028a55ac40ce5df4b0c7441d51a843e3fc,9000000000000000000000000000" \
  --account="0xcb5158eccfe180f35d4c05483cfaf3f769210ade3fd1f281737d10d491b5be40,9000000000000000000000000000" \
