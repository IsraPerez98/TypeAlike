//! The dictionaries of `wooorm/dictionaries` at the commit [`COMMIT`], with the size and the
//! SHA-256 digest of each file, from `make_catalog.py`.

use crate::catalog::{CatalogEntry, PinnedFile};

/// The commit of `wooorm/dictionaries` that each file comes from.
pub(crate) const COMMIT: &str = "8cfea406b505e4d7df52d5a19bce525df98c54ab";

/// Each dictionary, by its language.
pub(crate) const ENTRIES: [CatalogEntry; 89] = [
    CatalogEntry {
        language: "bg",
        folder: "bg",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 58_189,
                sha256: "cb6bfc56d6b1e70c812061086d7ec5ad6aaf6d35acd927a035f1cee0188ea477",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_566_242,
                sha256: "44408b1ee50301e8f9a133b0ec92565bb8be49818c444d91821876d447f58906",
            },
            PinnedFile {
                name: "license",
                size: 3_516,
                sha256: "04f8571e50b57ddcd93e74416e1949325c899b902af36e83fa669b6205f56b2e",
            },
        ],
    },
    CatalogEntry {
        language: "br",
        folder: "br",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 13_888,
                sha256: "8c05d6c1e2a9279f67407018145cf7f0b8b6da4f3a8ed83939a55f8e32af27cd",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_967_626,
                sha256: "235241afc01c0726d32427e1411f96bee1a5f536bd1bdb7f083a311e6f0d780b",
            },
            PinnedFile {
                name: "license",
                size: 1_547,
                sha256: "3fbedfd3144eaff8eddd3eeb4f06009f6fc6925b180d0bddb6eeba5c7403e4db",
            },
        ],
    },
    CatalogEntry {
        language: "ca",
        folder: "ca",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 49_922,
                sha256: "bfc7d8412c9450ec63e00d13f2153191a11d04868b14d51cf6c50eace71efdb2",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_876_608,
                sha256: "e272aeff4b2d1a92b428a22b52c685292d04936e34dfe7786e27539175cbef2e",
            },
            PinnedFile {
                name: "license",
                size: 219,
                sha256: "0287b97b6dc8932f58d8d91dc8364aa0be78d6800517875a76624af7cc92782b",
            },
        ],
    },
    CatalogEntry {
        language: "ca-valencia",
        folder: "ca-valencia",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 50_122,
                sha256: "b4c50941df5a8e3116b706ceb6010b09a8e6b15a99e20c4f281fcbee2d2544e4",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_881_104,
                sha256: "80ada66b789096a526ab05d106e4658cd39bcc2c4442c4c893051bf4e8fa514c",
            },
            PinnedFile {
                name: "license",
                size: 219,
                sha256: "0287b97b6dc8932f58d8d91dc8364aa0be78d6800517875a76624af7cc92782b",
            },
        ],
    },
    CatalogEntry {
        language: "cs",
        folder: "cs",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 112_714,
                sha256: "175e02acb4786042a9c87e3cb6d3746e3a5b3079752118c2d4f0bb2c756ce72b",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_648_134,
                sha256: "bbc0a495671c29337279182799e171c173a21ae8968c89dd0fb7c713b40c7832",
            },
            PinnedFile {
                name: "license",
                size: 48_038,
                sha256: "1bd9054040365c2c749565d791dc8fb1659a2566996c57b1f85031f6489c4b84",
            },
        ],
    },
    CatalogEntry {
        language: "cy",
        folder: "cy",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 7_489,
                sha256: "757c350b84889c4d0711c93760509fe532863079262695c069550f9a9c1c5222",
            },
            PinnedFile {
                name: "index.dic",
                size: 888_651,
                sha256: "ef8f01db311cf17c617377d22b11f6752b01c65f5457820cc04d877bba6b92ff",
            },
            PinnedFile {
                name: "license",
                size: 1_439,
                sha256: "a36169950f39f4286fc7f941f48518ffba734b3e95de78356833c7ccd58ea2b2",
            },
        ],
    },
    CatalogEntry {
        language: "da",
        folder: "da",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 81_584,
                sha256: "cb5ef704de0ab8575c883c97981d5d242260c7ebe47b28c15c205443aca15976",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_719_300,
                sha256: "9a12345e1881c96f18cc460dbd062e47509571d5f2a2a9bf041da169856dc19e",
            },
            PinnedFile {
                name: "license",
                size: 787,
                sha256: "ef2785b76dbcb1d0b620435158624a9002b995ee4521cf5d41b055e29eb9d1af",
            },
        ],
    },
    CatalogEntry {
        language: "de",
        folder: "de",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 19_199,
                sha256: "57fdd1b16aac2131003c91e0cf2a488becb970382a402a9ce089307301cb3ef0",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_118_194,
                sha256: "b5c781a0cf6f285fb6b9b8ab02fbea104b987104a1efdda8a835837e89e3ec77",
            },
            PinnedFile {
                name: "license",
                size: 760,
                sha256: "03abf202c6e207d41f054083cafccbb174a30f6b330ea8dba3aeef0a279cb83e",
            },
        ],
    },
    CatalogEntry {
        language: "de-AT",
        folder: "de-AT",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 19_199,
                sha256: "04ebc966a0b1130d8fc62886bb801e77bf7d6a11accf205954c364d3c0c68864",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_121_822,
                sha256: "02cbbeb74af85497b861b5782413b6c8c8ffab5c9423ef2e92265599b9359870",
            },
            PinnedFile {
                name: "license",
                size: 760,
                sha256: "03abf202c6e207d41f054083cafccbb174a30f6b330ea8dba3aeef0a279cb83e",
            },
        ],
    },
    CatalogEntry {
        language: "de-CH",
        folder: "de-CH",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 19_199,
                sha256: "11d773a903ece02c71b5094d71dcf3afe27cad0383cfe063fddbde26b8443785",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_119_710,
                sha256: "abb1abcc1951abba255d4e6289e5d44293338ae29d1331dea16c75bd89a43e1c",
            },
            PinnedFile {
                name: "license",
                size: 760,
                sha256: "03abf202c6e207d41f054083cafccbb174a30f6b330ea8dba3aeef0a279cb83e",
            },
        ],
    },
    CatalogEntry {
        language: "el",
        folder: "el",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 20_058,
                sha256: "5d2e3580af6d3523ac4470335a4b05968e59dddc58e1a7f2b260c81342e80063",
            },
            PinnedFile {
                name: "index.dic",
                size: 19_421_967,
                sha256: "f08daefb302600beb1b345e4fd77f4ecf6617aa080a72efe6ae7eec0ad5b2ac7",
            },
            PinnedFile {
                name: "license",
                size: 2_238,
                sha256: "53970cbce0276c3cc4ba4fff842789e95fb167458d257568faa0ff5f68ffe61a",
            },
        ],
    },
    CatalogEntry {
        language: "el-polyton",
        folder: "el-polyton",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 18_678,
                sha256: "e734b5c534d9f5f48fc9edce2e75ac695b3620ebd1f9817cd43fda887f81107f",
            },
            PinnedFile {
                name: "index.dic",
                size: 16_669_155,
                sha256: "2368557909c26a368234623c199a70f4322e62a4c933dd32da43cb55bb26efa8",
            },
            PinnedFile {
                name: "license",
                size: 3_920,
                sha256: "702981347ca9ac2e660915a28627cd7a0388e7ce7b45a00500611867ca13493b",
            },
        ],
    },
    CatalogEntry {
        language: "en-US",
        folder: "en",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 3_086,
                sha256: "8ae1f19d4840d957728ad90555d5a8dff6cc5c046279c95ff0c00fc0a0136c7b",
            },
            PinnedFile {
                name: "index.dic",
                size: 551_762,
                sha256: "f0b1a234bd178bdd01875b2a392a9647f888b8fe879f79c52aae62c2759b3647",
            },
            PinnedFile {
                name: "license",
                size: 15_731,
                sha256: "2a7e8d8ae9e8facc84818546ae2a8d83aec5e9c80a675ff789acd1c338b53b3d",
            },
        ],
    },
    CatalogEntry {
        language: "en-AU",
        folder: "en-AU",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 3_086,
                sha256: "8ae1f19d4840d957728ad90555d5a8dff6cc5c046279c95ff0c00fc0a0136c7b",
            },
            PinnedFile {
                name: "index.dic",
                size: 554_336,
                sha256: "aa07c46571f306b79fc1bc534357ed357af15687381b26f891ba66e8a2caed89",
            },
            PinnedFile {
                name: "license",
                size: 15_731,
                sha256: "510a37a13ae92cd25989211834b342747d6b96bceb80c48301e4d15142fd6d0a",
            },
        ],
    },
    CatalogEntry {
        language: "en-CA",
        folder: "en-CA",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 3_086,
                sha256: "8ae1f19d4840d957728ad90555d5a8dff6cc5c046279c95ff0c00fc0a0136c7b",
            },
            PinnedFile {
                name: "index.dic",
                size: 551_583,
                sha256: "53096689870246a78fa408b1e7a8c49bc140bd466a84a384cfb509445fff7173",
            },
            PinnedFile {
                name: "license",
                size: 15_731,
                sha256: "5df9138713211272cc2da7e8595cf5898f4b9793fb67eb6ebff494de4052fdd6",
            },
        ],
    },
    CatalogEntry {
        language: "en-GB",
        folder: "en-GB",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 3_086,
                sha256: "8ae1f19d4840d957728ad90555d5a8dff6cc5c046279c95ff0c00fc0a0136c7b",
            },
            PinnedFile {
                name: "index.dic",
                size: 552_106,
                sha256: "869fe17ba4ee4b5401c60a666ee2d6a3dcc237f460b7294435df1cc6a799aa57",
            },
            PinnedFile {
                name: "license",
                size: 15_739,
                sha256: "81b644347b40804c25811267efbea024a30b2bb1642fefa9c9c50f7b0bbcb67f",
            },
        ],
    },
    CatalogEntry {
        language: "en-ZA",
        folder: "en-ZA",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 35_805,
                sha256: "d39436eedc236c02e5d2b904352a51c487e421ca33e803f500713b5a21c007d1",
            },
            PinnedFile {
                name: "index.dic",
                size: 596_444,
                sha256: "cde2b8460049ec0f87933f56b20396fbb693e322f852b4878706eb5b2acfd768",
            },
            PinnedFile {
                name: "license",
                size: 32_927,
                sha256: "15f9c6bc800d53d7b6be0b659f326096533fbf0182209d2e5d9212987f681521",
            },
        ],
    },
    CatalogEntry {
        language: "eo",
        folder: "eo",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 16_322,
                sha256: "a668bb25baccb39a758692a94325c47195a70fe723fe9875d1b3a3b4545412a5",
            },
            PinnedFile {
                name: "index.dic",
                size: 382_137,
                sha256: "75b09812254cd6b49c2849ed3b92f1c252117906f5a2ce281d25e1798c437ac1",
            },
            PinnedFile {
                name: "license",
                size: 18_011,
                sha256: "e6d6a009505e345fe949e1310334fcb0747f28dae2856759de102ab66b722cb4",
            },
        ],
    },
    CatalogEntry {
        language: "es",
        folder: "es",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 167_135,
                sha256: "892039d59db11d747061b81c3a92a52d48ba0e055b5835af1943cbd3c2fba85b",
            },
            PinnedFile {
                name: "index.dic",
                size: 706_202,
                sha256: "907f786a8ceb3456722b20ad91dd4dbe99c5c16e45015ea63155e36f26b06d2c",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "a21581497f07b5e550e438d26890ea2b1704492357de67740613bd589b1c652a",
            },
        ],
    },
    CatalogEntry {
        language: "es-AR",
        folder: "es-AR",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 169_460,
                sha256: "9d210c1a32da44bf8a0baa992c2bf3e9ae4730beb392c5e7e948948b48ff5474",
            },
            PinnedFile {
                name: "index.dic",
                size: 698_035,
                sha256: "f4c9cd45bd56ad670200798f271a33360692ab29bdbc15661ac98fedc03ef714",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "8ccf2973df0d0c75eebe90ec9b47f1abe5763ed46561329ee52127bfc2c8b1e9",
            },
        ],
    },
    CatalogEntry {
        language: "es-BO",
        folder: "es-BO",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 693_089,
                sha256: "8096a7bdd06aec4fdfab48586d0703da445eda8aaf9398bd210a96b3759f6d4e",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "6d3231b1c88af0607e6d82cf959933f8f5af3dc82075e06c932fee2cf8cdaf54",
            },
        ],
    },
    CatalogEntry {
        language: "es-CL",
        folder: "es-CL",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 695_714,
                sha256: "e52d2a4c4cefdd1a5fab9e66f13a13e2af7f41078781cf0b59bc2f285e90063a",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "d19c9f3b0196311bcf0afcb2fdd6d65251911da6af4533d3ddd5d7c2006d70e1",
            },
        ],
    },
    CatalogEntry {
        language: "es-CO",
        folder: "es-CO",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 158_014,
                sha256: "4dc9ddafc140e0fc27a40fe808ff3d77138e12bf3db5748832f03d780aadecfa",
            },
            PinnedFile {
                name: "index.dic",
                size: 742_304,
                sha256: "3fbc134bbdd2c6724e6f8cc5375bdaef05b33e59aefb007a2ed1864b5a4f6d8e",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "9220057fa1d872f6ffeea65f7cbf4d9ecc69bc9b276771f6eb77cf8dbdf41d2b",
            },
        ],
    },
    CatalogEntry {
        language: "es-CR",
        folder: "es-CR",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 691_139,
                sha256: "9e5f628c5e43cc185530f788e6ae25407ab1eae9b8dfae291f61b31517d25b57",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "309859a7f9926d31266855fcdd15a3571d4a41d8e2fec1bee0c4af3441ace472",
            },
        ],
    },
    CatalogEntry {
        language: "es-CU",
        folder: "es-CU",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 693_050,
                sha256: "277f32d936d004ab345d6c0c35bc21455fb0451fe395ebe9a7e085588a1f34be",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "f5155ed6464618a9d336976781dded6f04361b3caa88d054821c571f9737ec66",
            },
        ],
    },
    CatalogEntry {
        language: "es-DO",
        folder: "es-DO",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 689_843,
                sha256: "4ff3ecbe2087bc139e4b1e9580a0d1e28a5a875c85895ac48ac998612446e5ba",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "c1faefb4cdc9c64b0b7c1c63b5de7d01b511f8a741ce7da8f018e2c567dd4adc",
            },
        ],
    },
    CatalogEntry {
        language: "es-EC",
        folder: "es-EC",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 691_730,
                sha256: "05f56b308775ed9320f50e7107148385a7ded87dcb18d96c9dacba11817c6337",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "961383e65a6d7f4f3264111860c09af96878f7b10ff9e09f872cd3e6bf5a207b",
            },
        ],
    },
    CatalogEntry {
        language: "es-GT",
        folder: "es-GT",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "c8bdd478feef7731fa0e6c8f029ec6a62e7b32488c0d976e3b0edbcbbf4dd4e2",
            },
            PinnedFile {
                name: "index.dic",
                size: 690_768,
                sha256: "674714464aae795aaaf442a0b08cfef2a178e08c6e6313160bc32df96bf4de6a",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "16f6d22f0001197ef3c74ed77811c1d55b1c93256561fd6e78d2928e25eb7df2",
            },
        ],
    },
    CatalogEntry {
        language: "es-HN",
        folder: "es-HN",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 692_774,
                sha256: "9364e9c6d3d76793f502669e250338b257054ee0b37cb392aca0abe66381444b",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "4920441e227db1041cbc246bb6c3c7cfd87152bc49d2f4eb3e9e75f32ba24b25",
            },
        ],
    },
    CatalogEntry {
        language: "es-MX",
        folder: "es-MX",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 158_014,
                sha256: "4dc9ddafc140e0fc27a40fe808ff3d77138e12bf3db5748832f03d780aadecfa",
            },
            PinnedFile {
                name: "index.dic",
                size: 714_044,
                sha256: "bfb17f2e285505fbe7a2d8c1785906e7ed2cd7f6fd6182afb9d8ac0e7e16d202",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "6292642265f1e6e105550e1212c5931b9d106480a370868e95620faaa5848495",
            },
        ],
    },
    CatalogEntry {
        language: "es-NI",
        folder: "es-NI",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 691_690,
                sha256: "7ab17d5634d2e1bf4f9e7b4d124eb0c69079d61bf9590818a8dac2e8b7192337",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "e6f2c46e06b0fee4024a6eb44b3a31142e552b59b649f6c22326f1df48eae721",
            },
        ],
    },
    CatalogEntry {
        language: "es-PA",
        folder: "es-PA",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 690_426,
                sha256: "6ff67ff57bfed406a740815a7f1f58deeea0a88262b60e555b7442830b4334e7",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "1c3958f0d749c90877705c6d62dca16aace5bb81b8771e4fa438e2148c536215",
            },
        ],
    },
    CatalogEntry {
        language: "es-PE",
        folder: "es-PE",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 158_014,
                sha256: "4dc9ddafc140e0fc27a40fe808ff3d77138e12bf3db5748832f03d780aadecfa",
            },
            PinnedFile {
                name: "index.dic",
                size: 705_766,
                sha256: "ef7bcc2f62bfa4de1a6266ba4dd8a0d2f65b22e005a3946d9572fb7fcbbbc6f9",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "77948a170352cd29abd04e63daa91149907c0704af882c779a69b71e01358047",
            },
        ],
    },
    CatalogEntry {
        language: "es-PH",
        folder: "es-PH",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 169_457,
                sha256: "459fcfa76382eb2333a3c2833053b3c37bb92345add3f8ad61e94e4413402c40",
            },
            PinnedFile {
                name: "index.dic",
                size: 788_629,
                sha256: "7655fb1ed746387c92f69e41fefad229c0c8715154606c673fb38a6de958c588",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "1deef2b8a2331faf3c6462e552a7403bf216f7689b7c8414324dff5da343dc46",
            },
        ],
    },
    CatalogEntry {
        language: "es-PR",
        folder: "es-PR",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 689_965,
                sha256: "7da63fd40658d0487862db0da3b42bf3046322eb68390ff123595d14732bfde4",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "9620457e871cee7ec84db935907cee525f49f0a84d693655b2877ec5d2bbed0b",
            },
        ],
    },
    CatalogEntry {
        language: "es-PY",
        folder: "es-PY",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 690_557,
                sha256: "3bcce5995c0f3382ded300ceb9a0fd48b47e04fb1bc516d8a0f42dc3934457eb",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "ebc2dfe48b05e46a62ba6a15ca554fffbfbdf11eb18b05060e6bfd800c2ae6c5",
            },
        ],
    },
    CatalogEntry {
        language: "es-SV",
        folder: "es-SV",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 691_130,
                sha256: "7642f7cb031166ae3960e2354f98c80ab7511c57690d0c9a43ca70a4ac28672d",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "9da80bea94cd60fd3a7b5b95d83ded49f409546fa3b5ff88a797355ea63ca578",
            },
        ],
    },
    CatalogEntry {
        language: "es-US",
        folder: "es-US",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 169_457,
                sha256: "459fcfa76382eb2333a3c2833053b3c37bb92345add3f8ad61e94e4413402c40",
            },
            PinnedFile {
                name: "index.dic",
                size: 686_440,
                sha256: "37e8e1cb67121f37a622fc559ffb80bab26144883bf723bbfe184e8645215dc0",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "51e08ed1037e3db5dbb372fde3e7749727cf2424635e9250a4469a8d5ffd8077",
            },
        ],
    },
    CatalogEntry {
        language: "es-UY",
        folder: "es-UY",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 160_328,
                sha256: "7b52471a40dde29f06dc5ca5071fd4124225ad1dedaaaa932009eb4acc4e1bb7",
            },
            PinnedFile {
                name: "index.dic",
                size: 692_682,
                sha256: "8ed4cc0c6a95dca239312a99b21d860c0c6cc93e5ad70f44cf34ec6cf8970a8a",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "6c7e32822f8d8ee95187fd769c334714a5489e26a0ea01b825e1b4287ffa0420",
            },
        ],
    },
    CatalogEntry {
        language: "es-VE",
        folder: "es-VE",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 158_014,
                sha256: "4dc9ddafc140e0fc27a40fe808ff3d77138e12bf3db5748832f03d780aadecfa",
            },
            PinnedFile {
                name: "index.dic",
                size: 694_769,
                sha256: "1a855f3a06e48a341f4d509729cfd65a309bf544d90faef67499071226f482de",
            },
            PinnedFile {
                name: "license",
                size: 2_596,
                sha256: "1f3804889644c810fa13561417dac5fb1fe6b5477cc45ba2b44f36bb718193ae",
            },
        ],
    },
    CatalogEntry {
        language: "et",
        folder: "et",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 240_016,
                sha256: "f568a1003d9d49d377e87e35a4b712a243b4490ef0647992c85169d46ed948e3",
            },
            PinnedFile {
                name: "index.dic",
                size: 4_457_441,
                sha256: "237ff31093ec03c7f992919b8e7c9a4d3df90cdef9859edab571ebff42c9f5e8",
            },
            PinnedFile {
                name: "readme.md",
                size: 2_894,
                sha256: "4a0065d329249b2172c4cb35a3ed9b04fe903639b6b231f7b54615366846bef9",
            },
        ],
    },
    CatalogEntry {
        language: "eu",
        folder: "eu",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 2_729_393,
                sha256: "ba68948c642828c80e5b5fc7ea19876bd38ce6f874f4453c662395adca8fef2a",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_303_418,
                sha256: "2f046ed070c11f9dc6f502577462bdc11a0966be50e0d9750823922a22d9e948",
            },
            PinnedFile {
                name: "readme.md",
                size: 2_885,
                sha256: "f2fb6de196e25ae7ce894d8d989263dae3718e9736c7ebd165cea5c4f7b7d762",
            },
        ],
    },
    CatalogEntry {
        language: "fa",
        folder: "fa",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 5_440,
                sha256: "dba0980816354ad969aa46dda0c040eeebda4a9381be331e17eae8647a2a1e1c",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_575_990,
                sha256: "314f31e059994f26e8016c834b14dfd10f74834b3f623a6040265b8f2a9dddab",
            },
            PinnedFile {
                name: "license",
                size: 582,
                sha256: "7429736f069a84049bcfbee436e5cea3a3fd9b98348d880d9e43ebcf39721989",
            },
        ],
    },
    CatalogEntry {
        language: "fo",
        folder: "fo",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 170_213,
                sha256: "6e459e5a3346ee0b7ad3ddc98dd0a412226060c6e7be6e096cfdf039f877b491",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_716_099,
                sha256: "3b62983af6b5c093e2ec020ff35869cc3068167e7cc7f8cc9585456a6b1368db",
            },
            PinnedFile {
                name: "license",
                size: 265,
                sha256: "8caace3ffac21939846af661b8955dea3419749ff45b9f7c9a80690cf6d6c49f",
            },
        ],
    },
    CatalogEntry {
        language: "fr",
        folder: "fr",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 199_870,
                sha256: "05a735d34c912e4e381ff08ee7c747923ccf5cf9dca81d8467982fa1ca51c2b7",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_229_133,
                sha256: "984e933237bc1224a48f42828233be9b03228260ef67aa8e2bdddcd03a26230d",
            },
            PinnedFile {
                name: "license",
                size: 2_758,
                sha256: "0c785aebc7aae8aa00b8163e9b3219a8785a9900b71e85475204e48a5651ced4",
            },
        ],
    },
    CatalogEntry {
        language: "fur",
        folder: "fur",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 17_832,
                sha256: "9b59cc525dec3a8580259e94493fd450a409835a0c35a19ac64b86fe9b750ec9",
            },
            PinnedFile {
                name: "index.dic",
                size: 384_726,
                sha256: "f1ac6d1029915bc749c8b4f4d454a28c977992328c541bd362266b6fc630988f",
            },
            PinnedFile {
                name: "license",
                size: 15_245,
                sha256: "95b65c7414e646b1a41f2ed230a358299cb4b1c06ae3952ffa380d4b54b289df",
            },
        ],
    },
    CatalogEntry {
        language: "fy",
        folder: "fy",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 7_020_461,
                sha256: "10264e8cf0a21a448da15840422382c44c6f7592070a1cb8c103cf4f19ed03fd",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_612_120,
                sha256: "529dd768a52b833fd3be90074dfeaaace18a785bd881e3d92ec1691312b901b1",
            },
            PinnedFile {
                name: "license",
                size: 35_149,
                sha256: "3972dc9744f6499f0f9b2dbf76696f2ae7ad8af9b23dde66d6af86c9dfb36986",
            },
        ],
    },
    CatalogEntry {
        language: "ga",
        folder: "ga",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 20_537,
                sha256: "1c48edbeb0e4225e612bb92d21f819fec58c9121b236db5ac0bba458982623b7",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_582_329,
                sha256: "3ca59ca30da8875020f83697d9f817d44b4efa02d78fe9fb52a2c5a86527349d",
            },
            PinnedFile {
                name: "license",
                size: 19_096,
                sha256: "37b50f1f97cae81339300b3d69f412f4ebb952b9f5ff09c66a3bccbd0ad57e36",
            },
        ],
    },
    CatalogEntry {
        language: "gd",
        folder: "gd",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 11_019,
                sha256: "88bbdc85cc1826fe91a2001dc1b48c96bce9a5e4a8c0c63169dc64c65a87cee9",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_361_638,
                sha256: "1769eb3317c90b9aa9e34b9ec95292d1ddde47115ed57c804a01eb03d9a0d83c",
            },
            PinnedFile {
                name: "license",
                size: 36_041,
                sha256: "1e24c0289727648357cf17a65988acaa79bbe784b7a7d56f41d4a0b7c86cd9ab",
            },
        ],
    },
    CatalogEntry {
        language: "gl",
        folder: "gl",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 1_159_910,
                sha256: "e2abc4122548c5f9e914fbbbcbebad3486b1492ac352fef72b95be93e32a0617",
            },
            PinnedFile {
                name: "index.dic",
                size: 8_636_406,
                sha256: "ca5f8d735a13225c1dabab042ddac8c58fd5e820ffba807ba8024d0fa0a68170",
            },
            PinnedFile {
                name: "license",
                size: 35_147,
                sha256: "8ceb4b9ee5adedde47b31e975c1d90c73ad27b6b165a1dcd80c7c545eb65b903",
            },
        ],
    },
    CatalogEntry {
        language: "he",
        folder: "he",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 78_883,
                sha256: "e115a0a5014d17a8e904f9d4a67d7992039b396c5fa031eb036a13bf63dfb037",
            },
            PinnedFile {
                name: "index.dic",
                size: 5_688_111,
                sha256: "9bd95042a927e13ab422231e355caa8a7018456e7c49b889f2e6c62ddb5a0552",
            },
            PinnedFile {
                name: "license",
                size: 461,
                sha256: "bb0a75c473e49224e0428b95c17ad90f1774a7f6cb97be921f9d6a93a3af80d7",
            },
        ],
    },
    CatalogEntry {
        language: "hr",
        folder: "hr",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 96_099,
                sha256: "230a7f0ca65159ecf8f3eea4afd79f7436415e83129b1d6b2424892074d8194d",
            },
            PinnedFile {
                name: "index.dic",
                size: 732_589,
                sha256: "fb436f3a758cd4faf9ef4cd5c2e6e2a1e3abe535ee183095a2f59fd18dad0731",
            },
            PinnedFile {
                name: "license",
                size: 153,
                sha256: "4edabc6989973eea67423ff9dedd133b33d032a09d821c70acaa43f49184e96c",
            },
        ],
    },
    CatalogEntry {
        language: "hu",
        folder: "hu",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 807_888,
                sha256: "da5cecde3c1afb67990030f1ffe8fb9859983c9305fc207abeb5475f8f002237",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_153_800,
                sha256: "3ac7aa0a100cabadddb4ace6c72ea3dcd6a5535aef63c8bf94f14073a71decd5",
            },
            PinnedFile {
                name: "license",
                size: 6_919,
                sha256: "28df62764d790d0b4dd322a517f5a468f952ba03b9ea6ff5c18a45d0f2faf2d9",
            },
        ],
    },
    CatalogEntry {
        language: "hy",
        folder: "hy",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 25_543,
                sha256: "aa8b49f32350a0136d112f62ee697c87cdc452256d12a70ef1fea2e7a13b32fb",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_210_856,
                sha256: "53530792b4937e4b8cd387e28ff45884e42742c8218d8ba4510804efcfb81084",
            },
            PinnedFile {
                name: "license",
                size: 353,
                sha256: "4471b1cc5e45985780f3c044dcfc3725b7845f3108309aa2a7be2df5c0510d32",
            },
        ],
    },
    CatalogEntry {
        language: "hyw",
        folder: "hyw",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 96_878,
                sha256: "4223fd49fb82891911f65270982beb5b929e44763476895706567596819dcd3e",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_068_288,
                sha256: "ecfbc8af694532bfb2a40654608d8e0d6cad0b55520543d84c97d2ae8b710694",
            },
            PinnedFile {
                name: "license",
                size: 353,
                sha256: "4471b1cc5e45985780f3c044dcfc3725b7845f3108309aa2a7be2df5c0510d32",
            },
        ],
    },
    CatalogEntry {
        language: "ia",
        folder: "ia",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 34_025,
                sha256: "08fa657fc641f6b2db0e9179d4219a14a0e7091a23471b794a5c5e9805283d18",
            },
            PinnedFile {
                name: "index.dic",
                size: 713_481,
                sha256: "22ee44726f058c904541b83db9bc3f4652e51186188fadd475619e3a596ada47",
            },
            PinnedFile {
                name: "license",
                size: 10_496,
                sha256: "6aa5a02caa670f3073868d681dd9626ab69c21a3f54426d317c97be677a2295f",
            },
        ],
    },
    CatalogEntry {
        language: "is",
        folder: "is",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 309_734,
                sha256: "0168050c64a369c55b5aeb61f9dfe484d2aa7512d78bb3fa36779ea24b467c50",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_454_138,
                sha256: "8447594fd7eb1d6d43725c4ca49b00d3f00c4c4f3ac2f2b6dfe3f56eb211d280",
            },
            PinnedFile {
                name: "license",
                size: 34_801,
                sha256: "a953a7bd6a7e45caf78dfff9ccce6a9f1e3ea2c2808ad88af28cce3890fad875",
            },
        ],
    },
    CatalogEntry {
        language: "it",
        folder: "it",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 70_020,
                sha256: "5770cd3e16d494c045b4a9a4a9fcd7962577e642d0384a7129c020a12cdd2c79",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_295_754,
                sha256: "b1348fbdb6f441ea9dd7e33b2cfcb96ead39ccd5e48bf894972774cd5aa86abb",
            },
            PinnedFile {
                name: "license",
                size: 38_596,
                sha256: "e48a7f7b3ec37ebb2c0d625528a7a677818de039bbae43e8f2c1dce6fd2a13e6",
            },
        ],
    },
    CatalogEntry {
        language: "ka",
        folder: "ka",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 63_577,
                sha256: "f5c39986659f13d703d0bae0ac8856d58768ce2b8d1f5ab6fb1ca3487c5ede5a",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_884_579,
                sha256: "5963f268e996f1e27e17163873b4d32f41fae2a1e5bef0117126caefe62f5857",
            },
            PinnedFile {
                name: "license",
                size: 1_036,
                sha256: "583322a252b4aef50d55c3300e42391697c406be5a732530428b2af3f0fa79ed",
            },
        ],
    },
    CatalogEntry {
        language: "ko",
        folder: "ko",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 11_094_418,
                sha256: "7b8930ce1357691591a3c38afc9cba787ec6dce0e30d0438bd156c4b4c94dbdf",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_864_525,
                sha256: "ea275b5fe256925a95f9192a173f7c3d3c2480fa748371c64b89d7e0cc531b1d",
            },
            PinnedFile {
                name: "license",
                size: 2_598,
                sha256: "3e81dd90f6d8877ccb2ed2b262ce72ee8a6ccf3a21179a8949abf0d85ac7fda0",
            },
        ],
    },
    CatalogEntry {
        language: "la",
        folder: "la",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 441_477,
                sha256: "6d78952f21ddd6158227544d573958a3d05b759c6219a2e1ac924daf24210e71",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_710_586,
                sha256: "8e0910e623800110cf9cc70b6adaea813857d02916b7c9a7dd9d9e9dd72a98ef",
            },
            PinnedFile {
                name: "license",
                size: 22_076,
                sha256: "2a1c6cdc97e95d92c8c9efee32d4e08224bc87549abb52f1eeec66684ecd4f72",
            },
        ],
    },
    CatalogEntry {
        language: "lb",
        folder: "lb",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 79_066,
                sha256: "0f32ab1e7e9e3a0e6f8ddf2d062897291587d8db60f6b04d0bfbcac14965a643",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_704_091,
                sha256: "fdcfb369bf6d4f38ad0ae3d831e40b55e9c0a97680f2f339e5bed9eecccd96fc",
            },
            PinnedFile {
                name: "license",
                size: 13_593,
                sha256: "86c11627556279536de1f4b440d4934fff9b17527072dda560d256eca016a8b5",
            },
        ],
    },
    CatalogEntry {
        language: "lt",
        folder: "lt",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 99_714,
                sha256: "79c6f8304490d1c8b27ec93e509b2f635d9bb21d45a2d41c15162b18c8ff897c",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_181_184,
                sha256: "a751750b040623cf408cf1a3b6f6336141b55ac9519e54068dbeaa52eb395ccd",
            },
            PinnedFile {
                name: "license",
                size: 1_520,
                sha256: "c5c73b4e3743ed2b959c70e886c4acee20d8654002ccdabac0a51af977ee2a17",
            },
        ],
    },
    CatalogEntry {
        language: "ltg",
        folder: "ltg",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 210_162,
                sha256: "ffe84b7e9fca7157d4f423f246ac3f4da42a78e41d21c64886f9632df00b35ff",
            },
            PinnedFile {
                name: "index.dic",
                size: 134_293,
                sha256: "0f99a93abe86adfe8304967e9e3e8af45dc0d39ba734b42dd49243acf6f8e7fd",
            },
            PinnedFile {
                name: "license",
                size: 2_315,
                sha256: "7537d31ea8c4c4ba2878dc3df3bd59c8a98ef1cf9e817b83fcd9ce884d87d633",
            },
        ],
    },
    CatalogEntry {
        language: "lv",
        folder: "lv",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 130_475,
                sha256: "a11f646c6905d6c96f3218ada4a6cf84129c9e6169720ac1a17f1707940a2890",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_844_831,
                sha256: "bf7d054e28bc5e44df19410f3ed9d5fd8d6a4d8c051676650388c4e8d65c3148",
            },
            PinnedFile {
                name: "license",
                size: 4_887,
                sha256: "9bfd51d5240995d95fe159be73bd34369dd9b97efc501118fd675e46c6089bdb",
            },
        ],
    },
    CatalogEntry {
        language: "mk",
        folder: "mk",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 181_552,
                sha256: "4774871b7cafa2d59bd2f539be7a2293f4aa2b2e2a331f1d11f8bcb849b4105e",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_279_845,
                sha256: "5008053d3b4e4be163c32b23a9d0551ae7aefceaa7d0760c07f2ebc10af7afb5",
            },
            PinnedFile {
                name: "license",
                size: 36_073,
                sha256: "1b8e0dea2caf07ba90b923725e06f9a50f2de9ead6667421f67c16a296abf967",
            },
        ],
    },
    CatalogEntry {
        language: "mn",
        folder: "mn",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 490_281,
                sha256: "a9cca137eecec85b99894abdbe1935ebf9bccae5767f9736538741c72818a5a5",
            },
            PinnedFile {
                name: "index.dic",
                size: 17_417_591,
                sha256: "0fe948e7d1824596fd9e6b4f7ee319c4fc2d8cc3e7fb7fea4961bf33e82adfd3",
            },
            PinnedFile {
                name: "license",
                size: 1_640,
                sha256: "c3ca161e7ae7b0f987d36d9f9f53fe285af52ca97dd397ce0eabf25ab56095bf",
            },
        ],
    },
    CatalogEntry {
        language: "nb",
        folder: "nb",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 17_381,
                sha256: "515a1e5f32ffa555d253c827830420dc5bb1d9a59ea9f720729acb0874c6a8c3",
            },
            PinnedFile {
                name: "index.dic",
                size: 5_349_724,
                sha256: "e97fdf7f441b3c46e1d0761a8ad25fa1883d0977d70ea8cb04751cb1c47fa10e",
            },
            PinnedFile {
                name: "license",
                size: 284,
                sha256: "583b1f60fd05cd7dec21293ebfb1f9f1d5b50d217521ef4f76287b50f2824770",
            },
        ],
    },
    CatalogEntry {
        language: "nds",
        folder: "nds",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 12_733,
                sha256: "3bede75cd7db3b8262778e6a5cc673bfee37cb703702e412682d806429ae1073",
            },
            PinnedFile {
                name: "index.dic",
                size: 103_126,
                sha256: "d8c380bed2182b1ac36bcef33a9391d9d77232045aaf43b59fe1cb492f378a95",
            },
            PinnedFile {
                name: "license",
                size: 1_072,
                sha256: "72fb471f7d12cb48e8c4e2a2418fa5c86fa82ee97e55ad393427157f80574cda",
            },
        ],
    },
    CatalogEntry {
        language: "ne",
        folder: "ne",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 69_405,
                sha256: "0e07d48fbebb6cb7f40d686a9eb9540a949922360d71c6d423a0384ee65f3fc2",
            },
            PinnedFile {
                name: "index.dic",
                size: 876_717,
                sha256: "dfc130b2ccbaeee54a859bdc512c27107eb6efa6ae5167615a65df83377ba427",
            },
            PinnedFile {
                name: "license",
                size: 749,
                sha256: "c79e6ed649e5a3d465f3d76c71046bdd01c44b0c7ca3ff1c7c1a0e57fe24ee0e",
            },
        ],
    },
    CatalogEntry {
        language: "nl",
        folder: "nl",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 46_732,
                sha256: "b430d9cf5f4170d9e5072397927e1402b34aa877d1f4c8fbc00dab6370d87312",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_489_166,
                sha256: "be192fdc36efd39dde0fb9a5a113143e0a906782e81ed68a74a710edced378d0",
            },
            PinnedFile {
                name: "license",
                size: 25_817,
                sha256: "fd9e95c360245eab3c388885062820754785faa4123964994cfdecb950b71948",
            },
        ],
    },
    CatalogEntry {
        language: "nn",
        folder: "nn",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 13_615,
                sha256: "e50821655063f5680a3e4cf775b0a69c87de74bd6d2bbbb5da14ff8de4633beb",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_338_635,
                sha256: "eaf1dc7ee7c848358df9aa231640b054ca4e388f2a870185528e4b229626faa6",
            },
            PinnedFile {
                name: "license",
                size: 284,
                sha256: "85fbc8cb7925ce44e34d5c6a96abf2a8a45c4a66ab542044a6c827a44af9c526",
            },
        ],
    },
    CatalogEntry {
        language: "oc",
        folder: "oc",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 19_560,
                sha256: "aad666af536d132640941b2231287600dd226fcd432f6ca5c96281ac4bfbe13f",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_767_078,
                sha256: "986e7947c13682f4a56ac46737480475ec599b51e99a146e07d66f5ed2417b7f",
            },
            PinnedFile {
                name: "license",
                size: 980,
                sha256: "92580a2a92179284e7effe4be36aad1ec6fa8fcf9ee0716945048a4554d5932e",
            },
        ],
    },
    CatalogEntry {
        language: "pl",
        folder: "pl",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 268_824,
                sha256: "de7b54a8fa37562d58b481cf3d87a63e3dcc2d74021f6403eb0e4f5df9ecc225",
            },
            PinnedFile {
                name: "index.dic",
                size: 4_683_579,
                sha256: "74b6155391025419967c26913541b55b36d58e80b1028037d4a0b8ddcb18d0b7",
            },
            PinnedFile {
                name: "license",
                size: 2_138,
                sha256: "8bb92c291eb431ffe32f7c42a719a0b54acf2851690fe12ca292e248a12a37e6",
            },
        ],
    },
    CatalogEntry {
        language: "pt-BR",
        folder: "pt",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 979_789,
                sha256: "a9f3621b94eb4d6838474bf9982027758e77dcb248815f49409b1a4a477dbd27",
            },
            PinnedFile {
                name: "index.dic",
                size: 4_477_692,
                sha256: "32e2edd83541d58613bcc83ef731a79aadf85ed57e977a16c6b5f711a7a36cee",
            },
            PinnedFile {
                name: "license",
                size: 1_461,
                sha256: "bb064f3db7ba5f5536da7d079773f692c5f928822e8aebc3d78259ff6cceb9e5",
            },
        ],
    },
    CatalogEntry {
        language: "pt-PT",
        folder: "pt-PT",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 95_089,
                sha256: "975a209fcc892cb382fa5f34a28c391a39668661ce373ae071287809c5fcae24",
            },
            PinnedFile {
                name: "index.dic",
                size: 1_475_328,
                sha256: "fcac21564163586df74ba88cde362bb75560548b35bcaa780558c8f97f43de04",
            },
            PinnedFile {
                name: "license",
                size: 1_148,
                sha256: "a5d807381c41a7186b235f17cdf472f53d83831cb05a2bae3c44581cc818570c",
            },
        ],
    },
    CatalogEntry {
        language: "ro",
        folder: "ro",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 55_040,
                sha256: "3ee127a667bdec829faee546ae32033e8f10f7c63117f9b572ef03ad9df2b76b",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_202_394,
                sha256: "c26a9356f598a0ae89e7be650f6bdd9ba70acce66b41d7ab14c0c68639b6ed33",
            },
            PinnedFile {
                name: "license",
                size: 1_633,
                sha256: "854c3b7e6575da37d066d5a09945af336a8ecc62c5e3c1049e97504dc67cbd81",
            },
        ],
    },
    CatalogEntry {
        language: "ru",
        folder: "ru",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 71_236,
                sha256: "38ce7d4af78e211e9bafe4bf7e3d6a2c420591136cb738ec6648f8fdf6524cd7",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_473_191,
                sha256: "f6047416a0204adbecf3a451b874ec8a97ee37e2cbc714466ef04d8dbcc0d6fc",
            },
            PinnedFile {
                name: "license",
                size: 1_886,
                sha256: "262af2f6ad70a61e5ee1332ff44fa8ee50edca819cf33207d8ad6ba6a0c9be52",
            },
        ],
    },
    CatalogEntry {
        language: "rw",
        folder: "rw",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 38_203_967,
                sha256: "236049e5460d8b77c3a863bc2a32b0c81cc2cb5222d6a10efe8f5ba55a6a2feb",
            },
            PinnedFile {
                name: "index.dic",
                size: 542,
                sha256: "29d2e7fff8d8f94a800c4c8ca98ee8e3d1ef67d12ab18c84f91e408209b50f23",
            },
            PinnedFile {
                name: "license",
                size: 35_122,
                sha256: "12ac5047f2af0522f06798b1589ffc4599bc29c91f954d7874e0320634e777c0",
            },
        ],
    },
    CatalogEntry {
        language: "sk",
        folder: "sk",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 99_298,
                sha256: "a79dc992b849aa2355db38eea9c4722239993639d77bc81c9c1aa79f213e723c",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_502_147,
                sha256: "b6cdb6fb512c44881cc91329aea0b179840f5f9a3e6cb024c2ab8547f96f5d94",
            },
            PinnedFile {
                name: "license",
                size: 822,
                sha256: "04903300e57a64a9967f85cdc65038470165b56bd34843e161f89d1fb9bcc0cd",
            },
        ],
    },
    CatalogEntry {
        language: "sl",
        folder: "sl",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 15_054,
                sha256: "511043a4f9c64a966599e3d53fa1f4e9a172a87de4a7a0c2cf327098a3a605d1",
            },
            PinnedFile {
                name: "index.dic",
                size: 3_084_448,
                sha256: "5bac615dbd301ddbefbdaafdae242a89e46605916d77958df22778e770a95b9e",
            },
            PinnedFile {
                name: "license",
                size: 2_076,
                sha256: "80e3fb8a78bf7c3cf1c76679ff1f11d4cb100440c9362553636b02edca005eb8",
            },
        ],
    },
    CatalogEntry {
        language: "sr",
        folder: "sr",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 1_296,
                sha256: "10e11edbd55742975f0720b26a2b6669187e9b91fabc5063ee81f0db2f73279f",
            },
            PinnedFile {
                name: "index.dic",
                size: 4_447_970,
                sha256: "8bdbbe464f20a5a600ccad1ff0257883c42bb6170a55440d45b4b39e002f433a",
            },
            PinnedFile {
                name: "license",
                size: 1_412,
                sha256: "272cfe7e4b339cbad1750806139576f062f9dbb8a966a55acd3d8700c51daeca",
            },
        ],
    },
    CatalogEntry {
        language: "sr-Latn",
        folder: "sr-Latn",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 704,
                sha256: "9112fdbd53c8a20ac06397e736884450d61df0c6c5be6f5512210ed58229f939",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_489_827,
                sha256: "a41302b3e707c7a344560bc7df6db739db254f87be0442108a68f4ed2a34782d",
            },
            PinnedFile {
                name: "license",
                size: 888,
                sha256: "8f0c179268ac54bdbd9d2096eacde31aaf74383c02b50b3a99e71b4ebfc2611f",
            },
        ],
    },
    CatalogEntry {
        language: "sv",
        folder: "sv",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 18_581,
                sha256: "ff8ddb979daa9f128e7489ece5edd666ada7af787933221543b7cecb2b3ed478",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_344_199,
                sha256: "540ff8d0d9b05aae66dd919625d3a0bd25026d1e157027e6602e5dd11a07b645",
            },
            PinnedFile {
                name: "license",
                size: 378,
                sha256: "1314b6004bff32053ca25aca5fc419e905d75a6d49f9f9825baf4f02298af6d9",
            },
        ],
    },
    CatalogEntry {
        language: "sv-FI",
        folder: "sv-FI",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 18_581,
                sha256: "1a8fa2740a552b9d9fbc99c034d0a70eff80fa3f8a1c1603033867220220bd38",
            },
            PinnedFile {
                name: "index.dic",
                size: 2_339_364,
                sha256: "62874b2ca0080ae663c5e241940097161bb277e5b99bbc40f2cdda1548b38eaa",
            },
            PinnedFile {
                name: "license",
                size: 378,
                sha256: "1314b6004bff32053ca25aca5fc419e905d75a6d49f9f9825baf4f02298af6d9",
            },
        ],
    },
    CatalogEntry {
        language: "tk",
        folder: "tk",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 369_118,
                sha256: "c55a9e98cab463d6095920bc2f8f682837beb971c21a8ea37d8ef60d80133755",
            },
            PinnedFile {
                name: "index.dic",
                size: 784_836,
                sha256: "4c75d6e1c9f4e4780bd1a00828b37b8826232db4b204a96b30b841f94d2a0b81",
            },
            PinnedFile {
                name: "license",
                size: 11_357,
                sha256: "c71d239df91726fc519c6eb72d318ec65820627232b2f796219e87dcf35d0ab4",
            },
        ],
    },
    CatalogEntry {
        language: "tr",
        folder: "tr",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 240_814,
                sha256: "6a390c79b054ad427cf9a6bf6de43463953cc1918e7f727e9dd4ebd4f9702f42",
            },
            PinnedFile {
                name: "index.dic",
                size: 9_117_192,
                sha256: "dc1d8f7724f4aefd117274678de431e4ea1765f2e02e6983b6ca48cbf5a7fc7c",
            },
            PinnedFile {
                name: "license",
                size: 1_086,
                sha256: "c18fd6425a62f50a0f1d4ec1dd45611e2051a66c56c44349a5f9045325549c0c",
            },
        ],
    },
    CatalogEntry {
        language: "uk",
        folder: "uk",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 203_292,
                sha256: "4243327a3aa19c6db5a4fb2e0c4e3eba6ad26e7ccb8a3f4e634c28d9a2d18956",
            },
            PinnedFile {
                name: "index.dic",
                size: 8_493_094,
                sha256: "48cbe32567a2c3c98f83d437bcd84ad3c11310e9df4d6c15d888b1e67654be31",
            },
            PinnedFile {
                name: "license",
                size: 35_142,
                sha256: "d62f065830aa3739cc031156b9690805c7b2e811b4a178c8b4acd8725d561c94",
            },
        ],
    },
    CatalogEntry {
        language: "vi",
        folder: "vi",
        files: [
            PinnedFile {
                name: "index.aff",
                size: 1_081,
                sha256: "0d836ebf1d4f1e9964d931fbb76464b24037b71995c73784166a07c43e2a7ae5",
            },
            PinnedFile {
                name: "index.dic",
                size: 39_858,
                sha256: "9c56725b43e66de33d28aac8e58124e38247c8b2be34a8469b3113169110256c",
            },
            PinnedFile {
                name: "license",
                size: 456,
                sha256: "4765f3b113b57fec3fff1d790128b4dca78069bebb156bf0e241a29cf4fe3984",
            },
        ],
    },
];
