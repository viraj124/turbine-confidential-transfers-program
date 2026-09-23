use anchor_spl::token_interface::spl_token_2022::extension::ExtensionType;

pub const STABLECOIN_MINT_EXTENSIONS: [ExtensionType; 4] = [
    ExtensionType::TransferFeeConfig,
    ExtensionType::MetadataPointer,
    ExtensionType::DefaultAccountState,
    ExtensionType::MintCloseAuthority,
];

pub const CONFIDENTIAL_STABLECOIN_MINT_EXTENSIONS: [ExtensionType; 7] = [
    ExtensionType::TransferFeeConfig,
    ExtensionType::MetadataPointer,
    ExtensionType::DefaultAccountState,
    ExtensionType::MintCloseAuthority,
    ExtensionType::PermanentDelegate,
    ExtensionType::ConfidentialTransferMint,
    ExtensionType::ConfidentialTransferFeeConfig,
];

pub const CONFIDENTIAL_ACCOUNT_EXTENSIONS: [ExtensionType; 2] = [
    ExtensionType::ConfidentialTransferAccount,
    ExtensionType::ConfidentialTransferFeeAmount,
];

pub const MAXIMUM_PENDING_BALANCE_CREDIT_COUNTER: u64 = 65_536;
