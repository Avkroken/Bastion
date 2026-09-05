# Changelog

## [0.22.1](https://github.com/Avkroken/Bastion/compare/v0.22.0...v0.22.1) (2026-09-05)


### Documentation

* rename governance file to REPO.md ([#443](https://github.com/Avkroken/Bastion/issues/443)) ([385a63c](https://github.com/Avkroken/Bastion/commit/385a63ce7fd42eb017e4a25919c26a199ad425a1))

## [0.22.0](https://github.com/Avkroken/Bastion/compare/v0.21.0...v0.22.0) (2026-09-05)


### Features

* **android:** add launchable native SSH client ([#440](https://github.com/Avkroken/Bastion/issues/440)) ([252dbd6](https://github.com/Avkroken/Bastion/commit/252dbd6fee1f40a8c8a74290c0c5229120a54ed5))

## [0.21.0](https://github.com/Avkroken/Bastion/compare/v0.20.1...v0.21.0) (2026-09-05)


### Features

* **android:** detect silent SSH sessions ([#436](https://github.com/Avkroken/Bastion/issues/436)) ([fa75204](https://github.com/Avkroken/Bastion/commit/fa75204323d1827d870bd6b0e0d3c811040f099e))

## [0.20.1](https://github.com/Avkroken/Bastion/compare/v0.20.0...v0.20.1) (2026-09-05)


### Documentation

* add cross-platform feature conformance matrix ([#434](https://github.com/Avkroken/Bastion/issues/434)) ([312399e](https://github.com/Avkroken/Bastion/commit/312399eb45674dc62120124a0cf9c615eef6439c))

## [0.20.0](https://github.com/Avkroken/Bastion/compare/v0.19.2...v0.20.0) (2026-09-05)


### Features

* enable metadata-only AI issue triage ([#413](https://github.com/Avkroken/Bastion/issues/413)) ([5341161](https://github.com/Avkroken/Bastion/commit/53411618ca521df9978512065be03998a7a92fcf))


### Fixes

* avoid invalid reusable-workflow variable context ([#425](https://github.com/Avkroken/Bastion/issues/425)) ([5619632](https://github.com/Avkroken/Bastion/commit/561963284b195f89a568f321ddb966b359642cbd))
* avoid secret-scanning false positives in key parser fixtures ([#431](https://github.com/Avkroken/Bastion/issues/431)) ([3518413](https://github.com/Avkroken/Bastion/commit/351841398978e747793cba0bf7e3591aeb0ef443))
* derive PBKDF2 key without app-owned buffer ([#381](https://github.com/Avkroken/Bastion/issues/381)) ([e06f299](https://github.com/Avkroken/Bastion/commit/e06f29916ae058ccc0ce9a9c4e4c2c3993e03e1e))
* konvertera symlänkstestets -avläsning till test_user() ([b99d765](https://github.com/Avkroken/Bastion/commit/b99d76571589f9625139118a57758b774bf40386))
* reconcile Dependabot outside PR events ([#421](https://github.com/Avkroken/Bastion/issues/421)) ([1e2515a](https://github.com/Avkroken/Bastion/commit/1e2515ab24f386c50fe7322ed4c9f22f81b611bd))
* schedule PR metadata reconciliation ([#423](https://github.com/Avkroken/Bastion/issues/423)) ([6897026](https://github.com/Avkroken/Bastion/commit/6897026f394005d77cc5d955c81b4cb24accf397))
* **security:** avoid secret-bearing assertion output ([#388](https://github.com/Avkroken/Bastion/issues/388)) ([6f0df7a](https://github.com/Avkroken/Bastion/commit/6f0df7aaeb1bf6b18f65a454308de603cb2d8429))
* **security:** remediate issue [#377](https://github.com/Avkroken/Bastion/issues/377) ([#383](https://github.com/Avkroken/Bastion/issues/383)) ([6c83a17](https://github.com/Avkroken/Bastion/commit/6c83a17db36d7bca723addc461a6ca1c43ae117e))
* **security:** remove hard-coded crypto patterns ([#379](https://github.com/Avkroken/Bastion/issues/379)) ([f84c256](https://github.com/Avkroken/Bastion/commit/f84c25694f28ddad257a53215445c3352496de37))
* serialize metadata routing and reconciliation ([#426](https://github.com/Avkroken/Bastion/issues/426)) ([3cbc2e7](https://github.com/Avkroken/Bastion/commit/3cbc2e731849b9eae185c3d9607ecf6b9f91b0d0))
* use centrally resolved Gamnacken client ID ([5619632](https://github.com/Avkroken/Bastion/commit/561963284b195f89a568f321ddb966b359642cbd))


### Documentation

* align CI contract with merge queue ([24a3a21](https://github.com/Avkroken/Bastion/commit/24a3a21f4c8b9b1901d38ba5efda5c3e2fe69e5c))
* align CI documentation with merge queue ([#419](https://github.com/Avkroken/Bastion/issues/419)) ([24a3a21](https://github.com/Avkroken/Bastion/commit/24a3a21f4c8b9b1901d38ba5efda5c3e2fe69e5c))
* align CI merge policy ([#364](https://github.com/Avkroken/Bastion/issues/364)) ([21cbc4c](https://github.com/Avkroken/Bastion/commit/21cbc4c8c40d7b0fb61a59125ebf5039c6728267))
* centralize agent policy ([#416](https://github.com/Avkroken/Bastion/issues/416)) ([cbbc8e0](https://github.com/Avkroken/Bastion/commit/cbbc8e0c6fc811a99d684728cb9338f444c2da24))
* consolidate authoritative AI agent policy ([#390](https://github.com/Avkroken/Bastion/issues/390)) ([32cc69c](https://github.com/Avkroken/Bastion/commit/32cc69cb1768bc4cb25ca5f83930d7fb6fcc8f16))
* Copilot-beroende ruleset-regler hör inte hemma i standarden ([d276206](https://github.com/Avkroken/Bastion/commit/d276206d198b475d085b4b4ee6df996740d9ecdc))
* Copilot-reglerna är urkryssade i samtliga repon ([631f87c](https://github.com/Avkroken/Bastion/commit/631f87cc2a5b31e8f8777d9934f093eaeccef2a3))
* Dependabot är standarden i alla repon, inte Renovate ([f99b06e](https://github.com/Avkroken/Bastion/commit/f99b06e6f75bb681ef4c502ca2c69238e9834435))
* dokumentera projektflöde och sponsring ([81311ed](https://github.com/Avkroken/Bastion/commit/81311ed1b40dd9e1054f55495b0b850467aa491a))
* frys PR-scope efter öppning ([2a2f54e](https://github.com/Avkroken/Bastion/commit/2a2f54e857241defc4059d1a42187a6f2dccd9b0))
* inherit organization funding links ([#422](https://github.com/Avkroken/Bastion/issues/422)) ([273773a](https://github.com/Avkroken/Bastion/commit/273773a35e5e703ed336f58872307050ee5d060d))
* interaktiv gränssnittsprototyp för alla fem plattformar ([8ca5eea](https://github.com/Avkroken/Bastion/commit/8ca5eea5c0d7401b7fd38e781c647498caac4bf0))
* interaktiv gränssnittsprototyp för alla fem plattformar ([#344](https://github.com/Avkroken/Bastion/issues/344)) ([2a6c384](https://github.com/Avkroken/Bastion/commit/2a6c384ac90a18ed3c906082b4da90aa8f93f5ab))
* korrigera branchsynkning i guldstandarden ([7ad5898](https://github.com/Avkroken/Bastion/commit/7ad5898faca5be6d6815f11a68f0b995ac1c6452))
* rätta agent-reglerna som motsade praktiken ([e80903e](https://github.com/Avkroken/Bastion/commit/e80903e150cd30d438b2a0552546b9996de2cb08))
* rätta agent-reglerna som motsade praktiken ([#328](https://github.com/Avkroken/Bastion/issues/328)) ([7e8da28](https://github.com/Avkroken/Bastion/commit/7e8da28f5a10332b99d2bfef0572bae7703d827c))
* skärp PR-gates och auto-merge ([#382](https://github.com/Avkroken/Bastion/issues/382)) ([11f6285](https://github.com/Avkroken/Bastion/commit/11f6285930cea0a40b1e73b3517b07b794526d32))
* skilj på vilken Copilot-regel som faktiskt blockerar merge ([859431d](https://github.com/Avkroken/Bastion/commit/859431d6234fa652ce4b3560a7dc1fc0839913ad))
* skriv in svarsformatet från i-have-adhd i AGENTS.md ([bf06457](https://github.com/Avkroken/Bastion/commit/bf0645725033774c0738accda0fedb6f68314cc5))
* skriv in svarsformatet från i-have-adhd i AGENTS.md ([#326](https://github.com/Avkroken/Bastion/issues/326)) ([d938955](https://github.com/Avkroken/Bastion/commit/d93895575ac945b95c81f17df6033ae3cf2a56de))
* stäm av GULDSTANDARD mot de faktiska branch-rulesetsen ([2a08b8b](https://github.com/Avkroken/Bastion/commit/2a08b8bdbfff15ffe0eaf1580b5538c814e4fb8b))
* standardize bug issue form ([8d1ed66](https://github.com/Avkroken/Bastion/commit/8d1ed66e0640b33517c7f032531f42489946f5c3))
* unify community health files ([#420](https://github.com/Avkroken/Bastion/issues/420)) ([8d1ed66](https://github.com/Avkroken/Bastion/commit/8d1ed66e0640b33517c7f032531f42489946f5c3))
