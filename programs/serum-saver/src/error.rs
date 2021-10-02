use anchor_lang::prelude::*;

pub type SaverResult<T = ()> = Result<T>;

#[error]
pub enum SaverError {
    #[msg("Swap coin_qty is overflow")]
    CoinQtyOverflow,
    #[msg("Amount should be greater than zero")]
    NonZeroU64,
}
