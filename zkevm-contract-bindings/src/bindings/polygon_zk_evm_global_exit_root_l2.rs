pub use polygon_zk_evm_global_exit_root_l2::*;
/// This module was auto-generated with ethers-rs Abigen.
/// More information at: <https://github.com/gakonst/ethers-rs>
#[allow(
    clippy::enum_variant_names,
    clippy::too_many_arguments,
    clippy::upper_case_acronyms,
    clippy::type_complexity,
    dead_code,
    non_camel_case_types
)]
pub mod polygon_zk_evm_global_exit_root_l2 {
    #[rustfmt::skip]
    const __ABI: &str = "[{\"inputs\":[{\"internalType\":\"address\",\"name\":\"_bridgeAddress\",\"type\":\"address\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"constructor\",\"outputs\":[]},{\"inputs\":[],\"type\":\"error\",\"name\":\"OnlyAllowedContracts\",\"outputs\":[]},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"bridgeAddress\",\"outputs\":[{\"internalType\":\"address\",\"name\":\"\",\"type\":\"address\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"\",\"type\":\"bytes32\",\"components\":[]}],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"globalExitRootMap\",\"outputs\":[{\"internalType\":\"uint256\",\"name\":\"\",\"type\":\"uint256\",\"components\":[]}]},{\"inputs\":[],\"stateMutability\":\"view\",\"type\":\"function\",\"name\":\"lastRollupExitRoot\",\"outputs\":[{\"internalType\":\"bytes32\",\"name\":\"\",\"type\":\"bytes32\",\"components\":[]}]},{\"inputs\":[{\"internalType\":\"bytes32\",\"name\":\"newRoot\",\"type\":\"bytes32\",\"components\":[]}],\"stateMutability\":\"nonpayable\",\"type\":\"function\",\"name\":\"updateExitRoot\",\"outputs\":[]}]";
    ///The parsed JSON ABI of the contract.
    pub static POLYGONZKEVMGLOBALEXITROOTL2_ABI: ::ethers::contract::Lazy<
        ::ethers::core::abi::Abi,
    > = ::ethers::contract::Lazy::new(|| {
        ::ethers::core::utils::__serde_json::from_str(__ABI).expect("ABI is always valid")
    });
    #[rustfmt::skip]
    const __BYTECODE: &[u8] = &[
        96,
        160,
        96,
        64,
        82,
        52,
        128,
        21,
        97,
        0,
        16,
        87,
        96,
        0,
        128,
        253,
        91,
        80,
        96,
        64,
        81,
        97,
        2,
        14,
        56,
        3,
        128,
        97,
        2,
        14,
        131,
        57,
        129,
        1,
        96,
        64,
        129,
        144,
        82,
        97,
        0,
        47,
        145,
        97,
        0,
        64,
        86,
        91,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        22,
        96,
        128,
        82,
        97,
        0,
        112,
        86,
        91,
        96,
        0,
        96,
        32,
        130,
        132,
        3,
        18,
        21,
        97,
        0,
        82,
        87,
        96,
        0,
        128,
        253,
        91,
        129,
        81,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        129,
        22,
        129,
        20,
        97,
        0,
        105,
        87,
        96,
        0,
        128,
        253,
        91,
        147,
        146,
        80,
        80,
        80,
        86,
        91,
        96,
        128,
        81,
        97,
        1,
        126,
        97,
        0,
        144,
        96,
        0,
        57,
        96,
        0,
        129,
        129,
        96,
        167,
        1,
        82,
        96,
        236,
        1,
        82,
        97,
        1,
        126,
        96,
        0,
        243,
        254,
        96,
        128,
        96,
        64,
        82,
        52,
        128,
        21,
        97,
        0,
        16,
        87,
        96,
        0,
        128,
        253,
        91,
        80,
        96,
        4,
        54,
        16,
        97,
        0,
        76,
        87,
        96,
        0,
        53,
        96,
        224,
        28,
        128,
        99,
        1,
        253,
        144,
        68,
        20,
        97,
        0,
        81,
        87,
        128,
        99,
        37,
        123,
        54,
        50,
        20,
        97,
        0,
        109,
        87,
        128,
        99,
        51,
        214,
        36,
        125,
        20,
        97,
        0,
        141,
        87,
        128,
        99,
        163,
        197,
        115,
        235,
        20,
        97,
        0,
        162,
        87,
        91,
        96,
        0,
        128,
        253,
        91,
        97,
        0,
        90,
        96,
        1,
        84,
        129,
        86,
        91,
        96,
        64,
        81,
        144,
        129,
        82,
        96,
        32,
        1,
        91,
        96,
        64,
        81,
        128,
        145,
        3,
        144,
        243,
        91,
        97,
        0,
        90,
        97,
        0,
        123,
        54,
        96,
        4,
        97,
        1,
        47,
        86,
        91,
        96,
        0,
        96,
        32,
        129,
        144,
        82,
        144,
        129,
        82,
        96,
        64,
        144,
        32,
        84,
        129,
        86,
        91,
        97,
        0,
        160,
        97,
        0,
        155,
        54,
        96,
        4,
        97,
        1,
        47,
        86,
        91,
        97,
        0,
        225,
        86,
        91,
        0,
        91,
        97,
        0,
        201,
        127,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        129,
        86,
        91,
        96,
        64,
        81,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        144,
        145,
        22,
        129,
        82,
        96,
        32,
        1,
        97,
        0,
        100,
        86,
        91,
        51,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        127,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        22,
        20,
        97,
        1,
        42,
        87,
        96,
        64,
        81,
        99,
        180,
        147,
        101,
        221,
        96,
        224,
        27,
        129,
        82,
        96,
        4,
        1,
        96,
        64,
        81,
        128,
        145,
        3,
        144,
        253,
        91,
        96,
        1,
        85,
        86,
        91,
        96,
        0,
        96,
        32,
        130,
        132,
        3,
        18,
        21,
        97,
        1,
        65,
        87,
        96,
        0,
        128,
        253,
        91,
        80,
        53,
        145,
        144,
        80,
        86,
        254,
        162,
        100,
        105,
        112,
        102,
        115,
        88,
        34,
        18,
        32,
        133,
        159,
        187,
        226,
        44,
        223,
        253,
        93,
        58,
        171,
        103,
        13,
        217,
        146,
        46,
        177,
        199,
        175,
        173,
        170,
        234,
        10,
        97,
        159,
        52,
        179,
        203,
        146,
        136,
        74,
        204,
        228,
        100,
        115,
        111,
        108,
        99,
        67,
        0,
        8,
        17,
        0,
        51,
    ];
    ///The bytecode of the contract.
    pub static POLYGONZKEVMGLOBALEXITROOTL2_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__BYTECODE);
    #[rustfmt::skip]
    const __DEPLOYED_BYTECODE: &[u8] = &[
        96,
        128,
        96,
        64,
        82,
        52,
        128,
        21,
        97,
        0,
        16,
        87,
        96,
        0,
        128,
        253,
        91,
        80,
        96,
        4,
        54,
        16,
        97,
        0,
        76,
        87,
        96,
        0,
        53,
        96,
        224,
        28,
        128,
        99,
        1,
        253,
        144,
        68,
        20,
        97,
        0,
        81,
        87,
        128,
        99,
        37,
        123,
        54,
        50,
        20,
        97,
        0,
        109,
        87,
        128,
        99,
        51,
        214,
        36,
        125,
        20,
        97,
        0,
        141,
        87,
        128,
        99,
        163,
        197,
        115,
        235,
        20,
        97,
        0,
        162,
        87,
        91,
        96,
        0,
        128,
        253,
        91,
        97,
        0,
        90,
        96,
        1,
        84,
        129,
        86,
        91,
        96,
        64,
        81,
        144,
        129,
        82,
        96,
        32,
        1,
        91,
        96,
        64,
        81,
        128,
        145,
        3,
        144,
        243,
        91,
        97,
        0,
        90,
        97,
        0,
        123,
        54,
        96,
        4,
        97,
        1,
        47,
        86,
        91,
        96,
        0,
        96,
        32,
        129,
        144,
        82,
        144,
        129,
        82,
        96,
        64,
        144,
        32,
        84,
        129,
        86,
        91,
        97,
        0,
        160,
        97,
        0,
        155,
        54,
        96,
        4,
        97,
        1,
        47,
        86,
        91,
        97,
        0,
        225,
        86,
        91,
        0,
        91,
        97,
        0,
        201,
        127,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        129,
        86,
        91,
        96,
        64,
        81,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        144,
        145,
        22,
        129,
        82,
        96,
        32,
        1,
        97,
        0,
        100,
        86,
        91,
        51,
        96,
        1,
        96,
        1,
        96,
        160,
        27,
        3,
        127,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        0,
        22,
        20,
        97,
        1,
        42,
        87,
        96,
        64,
        81,
        99,
        180,
        147,
        101,
        221,
        96,
        224,
        27,
        129,
        82,
        96,
        4,
        1,
        96,
        64,
        81,
        128,
        145,
        3,
        144,
        253,
        91,
        96,
        1,
        85,
        86,
        91,
        96,
        0,
        96,
        32,
        130,
        132,
        3,
        18,
        21,
        97,
        1,
        65,
        87,
        96,
        0,
        128,
        253,
        91,
        80,
        53,
        145,
        144,
        80,
        86,
        254,
        162,
        100,
        105,
        112,
        102,
        115,
        88,
        34,
        18,
        32,
        133,
        159,
        187,
        226,
        44,
        223,
        253,
        93,
        58,
        171,
        103,
        13,
        217,
        146,
        46,
        177,
        199,
        175,
        173,
        170,
        234,
        10,
        97,
        159,
        52,
        179,
        203,
        146,
        136,
        74,
        204,
        228,
        100,
        115,
        111,
        108,
        99,
        67,
        0,
        8,
        17,
        0,
        51,
    ];
    ///The deployed bytecode of the contract.
    pub static POLYGONZKEVMGLOBALEXITROOTL2_DEPLOYED_BYTECODE: ::ethers::core::types::Bytes =
        ::ethers::core::types::Bytes::from_static(__DEPLOYED_BYTECODE);
    pub struct PolygonZkEVMGlobalExitRootL2<M>(::ethers::contract::Contract<M>);
    impl<M> ::core::clone::Clone for PolygonZkEVMGlobalExitRootL2<M> {
        fn clone(&self) -> Self {
            Self(::core::clone::Clone::clone(&self.0))
        }
    }
    impl<M> ::core::ops::Deref for PolygonZkEVMGlobalExitRootL2<M> {
        type Target = ::ethers::contract::Contract<M>;
        fn deref(&self) -> &Self::Target {
            &self.0
        }
    }
    impl<M> ::core::ops::DerefMut for PolygonZkEVMGlobalExitRootL2<M> {
        fn deref_mut(&mut self) -> &mut Self::Target {
            &mut self.0
        }
    }
    impl<M> ::core::fmt::Debug for PolygonZkEVMGlobalExitRootL2<M> {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            f.debug_tuple(stringify!(PolygonZkEVMGlobalExitRootL2))
                .field(&self.address())
                .finish()
        }
    }
    impl<M: ::ethers::providers::Middleware> PolygonZkEVMGlobalExitRootL2<M> {
        /// Creates a new contract instance with the specified `ethers` client at
        /// `address`. The contract derefs to a `ethers::Contract` object.
        pub fn new<T: Into<::ethers::core::types::Address>>(
            address: T,
            client: ::std::sync::Arc<M>,
        ) -> Self {
            Self(::ethers::contract::Contract::new(
                address.into(),
                POLYGONZKEVMGLOBALEXITROOTL2_ABI.clone(),
                client,
            ))
        }
        /// Constructs the general purpose `Deployer` instance based on the provided constructor arguments and sends it.
        /// Returns a new instance of a deployer that returns an instance of this contract after sending the transaction
        ///
        /// Notes:
        /// - If there are no constructor arguments, you should pass `()` as the argument.
        /// - The default poll duration is 7 seconds.
        /// - The default number of confirmations is 1 block.
        ///
        ///
        /// # Example
        ///
        /// Generate contract bindings with `abigen!` and deploy a new contract instance.
        ///
        /// *Note*: this requires a `bytecode` and `abi` object in the `greeter.json` artifact.
        ///
        /// ```ignore
        /// # async fn deploy<M: ethers::providers::Middleware>(client: ::std::sync::Arc<M>) {
        ///     abigen!(Greeter, "../greeter.json");
        ///
        ///    let greeter_contract = Greeter::deploy(client, "Hello world!".to_string()).unwrap().send().await.unwrap();
        ///    let msg = greeter_contract.greet().call().await.unwrap();
        /// # }
        /// ```
        pub fn deploy<T: ::ethers::core::abi::Tokenize>(
            client: ::std::sync::Arc<M>,
            constructor_args: T,
        ) -> ::core::result::Result<
            ::ethers::contract::builders::ContractDeployer<M, Self>,
            ::ethers::contract::ContractError<M>,
        > {
            let factory = ::ethers::contract::ContractFactory::new(
                POLYGONZKEVMGLOBALEXITROOTL2_ABI.clone(),
                POLYGONZKEVMGLOBALEXITROOTL2_BYTECODE.clone().into(),
                client,
            );
            let deployer = factory.deploy(constructor_args)?;
            let deployer = ::ethers::contract::ContractDeployer::new(deployer);
            Ok(deployer)
        }
        ///Calls the contract's `bridgeAddress` (0xa3c573eb) function
        pub fn bridge_address(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::Address> {
            self.0
                .method_hash([163, 197, 115, 235], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `globalExitRootMap` (0x257b3632) function
        pub fn global_exit_root_map(
            &self,
            p0: [u8; 32],
        ) -> ::ethers::contract::builders::ContractCall<M, ::ethers::core::types::U256> {
            self.0
                .method_hash([37, 123, 54, 50], p0)
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `lastRollupExitRoot` (0x01fd9044) function
        pub fn last_rollup_exit_root(
            &self,
        ) -> ::ethers::contract::builders::ContractCall<M, [u8; 32]> {
            self.0
                .method_hash([1, 253, 144, 68], ())
                .expect("method not found (this should never happen)")
        }
        ///Calls the contract's `updateExitRoot` (0x33d6247d) function
        pub fn update_exit_root(
            &self,
            new_root: [u8; 32],
        ) -> ::ethers::contract::builders::ContractCall<M, ()> {
            self.0
                .method_hash([51, 214, 36, 125], new_root)
                .expect("method not found (this should never happen)")
        }
    }
    impl<M: ::ethers::providers::Middleware> From<::ethers::contract::Contract<M>>
        for PolygonZkEVMGlobalExitRootL2<M>
    {
        fn from(contract: ::ethers::contract::Contract<M>) -> Self {
            Self::new(contract.address(), contract.client())
        }
    }
    ///Custom Error type `OnlyAllowedContracts` with signature `OnlyAllowedContracts()` and selector `0xb49365dd`
    #[derive(
        Clone,
        ::ethers::contract::EthError,
        ::ethers::contract::EthDisplay,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[etherror(name = "OnlyAllowedContracts", abi = "OnlyAllowedContracts()")]
    pub struct OnlyAllowedContracts;
    ///Container type for all input parameters for the `bridgeAddress` function with signature `bridgeAddress()` and selector `0xa3c573eb`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "bridgeAddress", abi = "bridgeAddress()")]
    pub struct BridgeAddressCall;
    ///Container type for all input parameters for the `globalExitRootMap` function with signature `globalExitRootMap(bytes32)` and selector `0x257b3632`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "globalExitRootMap", abi = "globalExitRootMap(bytes32)")]
    pub struct GlobalExitRootMapCall(pub [u8; 32]);
    ///Container type for all input parameters for the `lastRollupExitRoot` function with signature `lastRollupExitRoot()` and selector `0x01fd9044`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "lastRollupExitRoot", abi = "lastRollupExitRoot()")]
    pub struct LastRollupExitRootCall;
    ///Container type for all input parameters for the `updateExitRoot` function with signature `updateExitRoot(bytes32)` and selector `0x33d6247d`
    #[derive(
        Clone,
        ::ethers::contract::EthCall,
        ::ethers::contract::EthDisplay,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    #[ethcall(name = "updateExitRoot", abi = "updateExitRoot(bytes32)")]
    pub struct UpdateExitRootCall {
        pub new_root: [u8; 32],
    }
    ///Container type for all of the contract's call
    #[derive(Clone, ::ethers::contract::EthAbiType, Debug, PartialEq, Eq, Hash)]
    pub enum PolygonZkEVMGlobalExitRootL2Calls {
        BridgeAddress(BridgeAddressCall),
        GlobalExitRootMap(GlobalExitRootMapCall),
        LastRollupExitRoot(LastRollupExitRootCall),
        UpdateExitRoot(UpdateExitRootCall),
    }
    impl ::ethers::core::abi::AbiDecode for PolygonZkEVMGlobalExitRootL2Calls {
        fn decode(
            data: impl AsRef<[u8]>,
        ) -> ::core::result::Result<Self, ::ethers::core::abi::AbiError> {
            let data = data.as_ref();
            if let Ok(decoded) = <BridgeAddressCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::BridgeAddress(decoded));
            }
            if let Ok(decoded) =
                <GlobalExitRootMapCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::GlobalExitRootMap(decoded));
            }
            if let Ok(decoded) =
                <LastRollupExitRootCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::LastRollupExitRoot(decoded));
            }
            if let Ok(decoded) =
                <UpdateExitRootCall as ::ethers::core::abi::AbiDecode>::decode(data)
            {
                return Ok(Self::UpdateExitRoot(decoded));
            }
            Err(::ethers::core::abi::Error::InvalidData.into())
        }
    }
    impl ::ethers::core::abi::AbiEncode for PolygonZkEVMGlobalExitRootL2Calls {
        fn encode(self) -> Vec<u8> {
            match self {
                Self::BridgeAddress(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::GlobalExitRootMap(element) => ::ethers::core::abi::AbiEncode::encode(element),
                Self::LastRollupExitRoot(element) => {
                    ::ethers::core::abi::AbiEncode::encode(element)
                }
                Self::UpdateExitRoot(element) => ::ethers::core::abi::AbiEncode::encode(element),
            }
        }
    }
    impl ::core::fmt::Display for PolygonZkEVMGlobalExitRootL2Calls {
        fn fmt(&self, f: &mut ::core::fmt::Formatter<'_>) -> ::core::fmt::Result {
            match self {
                Self::BridgeAddress(element) => ::core::fmt::Display::fmt(element, f),
                Self::GlobalExitRootMap(element) => ::core::fmt::Display::fmt(element, f),
                Self::LastRollupExitRoot(element) => ::core::fmt::Display::fmt(element, f),
                Self::UpdateExitRoot(element) => ::core::fmt::Display::fmt(element, f),
            }
        }
    }
    impl ::core::convert::From<BridgeAddressCall> for PolygonZkEVMGlobalExitRootL2Calls {
        fn from(value: BridgeAddressCall) -> Self {
            Self::BridgeAddress(value)
        }
    }
    impl ::core::convert::From<GlobalExitRootMapCall> for PolygonZkEVMGlobalExitRootL2Calls {
        fn from(value: GlobalExitRootMapCall) -> Self {
            Self::GlobalExitRootMap(value)
        }
    }
    impl ::core::convert::From<LastRollupExitRootCall> for PolygonZkEVMGlobalExitRootL2Calls {
        fn from(value: LastRollupExitRootCall) -> Self {
            Self::LastRollupExitRoot(value)
        }
    }
    impl ::core::convert::From<UpdateExitRootCall> for PolygonZkEVMGlobalExitRootL2Calls {
        fn from(value: UpdateExitRootCall) -> Self {
            Self::UpdateExitRoot(value)
        }
    }
    ///Container type for all return fields from the `bridgeAddress` function with signature `bridgeAddress()` and selector `0xa3c573eb`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct BridgeAddressReturn(pub ::ethers::core::types::Address);
    ///Container type for all return fields from the `globalExitRootMap` function with signature `globalExitRootMap(bytes32)` and selector `0x257b3632`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct GlobalExitRootMapReturn(pub ::ethers::core::types::U256);
    ///Container type for all return fields from the `lastRollupExitRoot` function with signature `lastRollupExitRoot()` and selector `0x01fd9044`
    #[derive(
        Clone,
        ::ethers::contract::EthAbiType,
        ::ethers::contract::EthAbiCodec,
        Default,
        Debug,
        PartialEq,
        Eq,
        Hash,
    )]
    pub struct LastRollupExitRootReturn(pub [u8; 32]);
}
