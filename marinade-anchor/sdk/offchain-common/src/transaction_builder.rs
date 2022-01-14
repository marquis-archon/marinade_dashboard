use std::sync::Arc;

use anyhow::Result;
use once_cell::sync::OnceCell;
use solana_sdk::{
    hash::Hash,
    instruction::Instruction,
    packet::PACKET_DATA_SIZE,
    program_pack::Pack,
    pubkey::Pubkey,
    rent::Rent,
    signature::{Signer, SignerError},
    system_instruction,
    transaction::Transaction,
};
use thiserror::Error;

use crate::signature_builder::SignatureBuilder;

pub struct PreparedTransaction {
    pub transaction: Transaction,
    pub signers: Vec<Arc<dyn Signer>>,
    pub instruction_descriptions: Vec<String>,
}

impl PreparedTransaction {
    pub fn new(
        transaction: Transaction,
        signature_builder: &SignatureBuilder,
        instruction_descriptions: Vec<String>,
    ) -> Result<Self, Pubkey> {
        let signers = signature_builder.signers_for_transaction(&transaction)?;
        Ok(Self {
            transaction,
            signers,
            instruction_descriptions,
        })
    }

    pub fn sign(&mut self, recent_blockhash: Hash) -> Result<&Transaction, SignerError> {
        self.transaction.try_sign(
            &self
                .signers
                .iter()
                .map(|arc| arc.as_ref())
                .collect::<Vec<_>>(),
            recent_blockhash,
        )?;
        Ok(&self.transaction)
    }

    pub fn into_signed(mut self, recent_blockhash: Hash) -> Result<Transaction, SignerError> {
        self.sign(recent_blockhash)?;
        Ok(self.transaction)
    }
}

#[derive(Debug, Clone, Error)]
pub enum TransactionBuildError {
    #[error("Unknown signer ${0}")]
    UnknownSigner(Pubkey),
    #[error("Too big transaction")]
    TooBigTransaction,
}

#[derive(Debug)]
pub struct TransactionBuilder {
    fee_payer: Pubkey,
    signature_builder: SignatureBuilder, // invariant: has signers for all instructions
    instruction_packs: Vec<Vec<(Instruction, String)>>,
    current_instruction_pack: OnceCell<Vec<(Instruction, String)>>,
    max_transaction_size: usize,
}

impl TransactionBuilder {
    pub fn new(fee_payer: Arc<dyn Signer>, max_transaction_size: usize) -> Self {
        let mut signature_builder = SignatureBuilder::default();
        Self {
            fee_payer: signature_builder.add_signer(fee_payer),
            signature_builder,
            instruction_packs: Vec::new(),
            current_instruction_pack: OnceCell::new(),
            max_transaction_size,
        }
    }

    pub fn fee_payer(&self) -> Pubkey {
        self.fee_payer
    }

    pub fn get_signer(&self, key: &Pubkey) -> Option<Arc<dyn Signer>> {
        self.signature_builder.get_signer(key)
    }

    pub fn fee_payer_signer(&self) -> Arc<dyn Signer> {
        self.get_signer(&self.fee_payer()).unwrap()
    }

    ///constructor, limit size to a single transaction
    pub fn limited(fee_payer: Arc<dyn Signer>) -> Self {
        Self::new(fee_payer, PACKET_DATA_SIZE)
    }

    ///constructor, no size limit, can be split in many transactions
    pub fn unlimited(fee_payer: Arc<dyn Signer>) -> Self {
        Self::new(fee_payer, 0)
    }

    pub fn add_signer(&mut self, signer: Arc<dyn Signer>) -> Pubkey {
        self.signature_builder.add_signer(signer)
    }

    pub fn new_signer(&mut self) -> Pubkey {
        self.signature_builder.new_signer()
    }

    fn check_signers(&self, instruction: &Instruction) -> Result<(), TransactionBuildError> {
        for account in &instruction.accounts {
            if account.is_signer && !self.signature_builder.contains_key(&account.pubkey) {
                return Err(TransactionBuildError::UnknownSigner(account.pubkey));
            }
        }
        Ok(())
    }

    #[inline]
    pub fn begin(&mut self) {
        self.current_instruction_pack
            .set(Vec::new())
            .expect("Double begin calls");
    }

    #[inline]
    pub fn commit(&mut self) {
        self.instruction_packs.push(
            self.current_instruction_pack
                .take()
                .expect("Commit without begin"),
        )
    }

    #[inline]
    pub fn rollback(&mut self) {
        self.current_instruction_pack
            .take()
            .expect("Rollback must be after begin");
    }

    #[inline]
    pub fn inside_transaction(&self) -> bool {
        self.current_instruction_pack.get().is_some()
    }

    #[inline]
    pub fn is_empty(&self) -> bool {
        (if let Some(current_instruction_pack) = self.current_instruction_pack.get() {
            current_instruction_pack.is_empty()
        } else {
            true
        }) && self.instruction_packs.is_empty()
    }

    #[inline]
    pub fn add_instruction(
        &mut self,
        instruction: Instruction,
        description: String,
    ) -> Result<(), TransactionBuildError> {
        self.check_signers(&instruction)?;
        let add_transaction = !self.inside_transaction();
        if add_transaction {
            self.begin();
        }
        let current = self.current_instruction_pack.get_mut().unwrap();

        current.push((instruction, description));
        let transaction_candidate = Transaction::new_with_payer(
            &current.iter().cloned().unzip::<_, _, Vec<_>, Vec<_>>().0,
            Some(&self.fee_payer),
        );
        if self.max_transaction_size > 0
            && bincode::serialize(&transaction_candidate).unwrap().len() > self.max_transaction_size
        {
            // Rollback
            if add_transaction {
                self.rollback();
            } else {
                current.pop();
            }
            return Err(TransactionBuildError::TooBigTransaction);
        }

        if add_transaction {
            self.commit();
        }
        Ok(())
    }

    pub fn build_next(&mut self) -> Option<PreparedTransaction> {
        assert!(
            self.current_instruction_pack
                .get()
                .map(Vec::is_empty)
                .unwrap_or(true),
            "Not committed transaction"
        );
        if !self.instruction_packs.is_empty() {
            let (instructions, descriptions): (Vec<Instruction>, Vec<String>) =
                self.instruction_packs.remove(0).into_iter().unzip();
            let transaction = Transaction::new_with_payer(&instructions, Some(&self.fee_payer));
            Some(
                PreparedTransaction::new(transaction, &self.signature_builder, descriptions)
                    .expect("Signature keys must be checked when instruction added"),
            )
        } else {
            None
        }
    }

    pub fn build_one(&mut self) -> PreparedTransaction {
        if let Some(transaction) = self.build_next() {
            assert!(self.instruction_packs.is_empty());
            transaction
        } else {
            panic!("Is not single transaction");
        }
    }

    pub fn build_next_combined(&mut self) -> Option<PreparedTransaction> {
        assert!(
            self.current_instruction_pack
                .get()
                .map(Vec::is_empty)
                .unwrap_or(true),
            "Not commited transaction"
        );
        if self.instruction_packs.is_empty() {
            return None;
        }
        let (transaction, descriptions) = if self.max_transaction_size == 0 {
            let (instructions, descriptions): (Vec<Instruction>, Vec<String>) =
                self.instruction_packs.drain(..).flatten().unzip();
            (
                Transaction::new_with_payer(&instructions, Some(&self.fee_payer)),
                descriptions,
            )
        } else {
            // One pack must fit transaction anyways
            let (mut instructions, mut descriptions): (Vec<Instruction>, Vec<String>) =
                self.instruction_packs.remove(0).into_iter().unzip();
            let mut transaction = Transaction::new_with_payer(&instructions, Some(&self.fee_payer));
            while let Some(next_pack) = self.instruction_packs.get(0) {
                let (next_instructions, next_descriptions): (Vec<Instruction>, Vec<String>) =
                    next_pack.iter().cloned().unzip();
                // Try to add next pack
                instructions.extend(next_instructions.into_iter());
                descriptions.extend(next_descriptions.into_iter());
                let transaction_candidate =
                    Transaction::new_with_payer(&instructions, Some(&self.fee_payer));

                if bincode::serialize(&transaction_candidate).unwrap().len()
                    <= self.max_transaction_size
                {
                    // Accept it
                    transaction = transaction_candidate;
                    // and move to the next pack
                    self.instruction_packs.remove(0);
                } else {
                    // Stop trying
                    break;
                }
            }
            (transaction, descriptions)
        };
        Some(
            PreparedTransaction::new(transaction, &self.signature_builder, descriptions)
                .expect("Signature keys must be checked when instruction added"),
        )
    }

    pub fn build_one_combined(&mut self) -> Option<PreparedTransaction> {
        if let Some(transaction) = self.build_next_combined() {
            assert!(self.is_empty(), "Not fit single transaction");
            Some(transaction)
        } else {
            None
        }
    }

    pub fn combined_sequence(&mut self) -> CombinedSequence {
        CombinedSequence { builder: self }
    }

    pub fn fit_into_single_transaction(&self) -> bool {
        let mut instructions: Vec<Instruction> = self
            .instruction_packs
            .iter()
            .flatten()
            .map(|(instruction, _description)| instruction.clone())
            .collect();
        if let Some(current_instructions) = self.current_instruction_pack.get() {
            instructions.extend(
                current_instructions
                    .iter()
                    .map(|(instruction, _description)| instruction.clone()),
            )
        }
        let transaction = Transaction::new_with_payer(&instructions, Some(&self.fee_payer));
        bincode::serialize(&transaction).unwrap().len() <= self.max_transaction_size
    }

    pub fn transfer_lamports(
        &mut self,
        source: Arc<dyn Signer>,
        target: &Pubkey,
        amount: u64,
        source_name: &str,
        target_name: &str,
    ) -> Result<(), TransactionBuildError> {
        self.add_signer(source.clone());
        self.add_instruction(
            system_instruction::transfer(&source.pubkey(), target, amount),
            format!(
                "Transfer {} lamports from {} {} to {} {}",
                amount,
                source.pubkey(),
                source_name,
                target,
                target_name
            ),
        )
    }

    pub fn create_account(
        &mut self,
        account: Arc<dyn Signer>,
        space: usize,
        owner: &Pubkey,
        rent: &Rent,
        name: &str,
    ) -> Result<(), TransactionBuildError> {
        self.add_signer(account.clone());
        self.add_instruction(
            system_instruction::create_account(
                &self.fee_payer(),
                &account.pubkey(),
                rent.minimum_balance(space),
                space as u64,
                owner,
            ),
            format!("Create {} {} with size {}", name, account.pubkey(), space),
        )
    }

    pub fn create_account_with_seed(
        &mut self,
        base: Arc<dyn Signer>,
        seed: &str,
        space: usize,
        owner: &Pubkey,
        rent: &Rent,
        name: &str,
    ) -> Result<Pubkey, TransactionBuildError> {
        let account = Pubkey::create_with_seed(&base.pubkey(), seed, owner).unwrap();
        self.add_instruction(
            system_instruction::create_account_with_seed(
                &self.fee_payer(),
                &account,
                &base.pubkey(),
                seed,
                rent.minimum_balance(space),
                space as u64,
                owner,
            ),
            format!(
                "Create {} {} with size {} with seed {} from {}",
                name,
                account,
                space,
                seed,
                base.pubkey()
            ),
        )?;
        Ok(account)
    }

    pub fn create_mint_account(
        &mut self,
        account: Arc<dyn Signer>,
        mint_authority: &Pubkey,
        freeze_authority: Option<&Pubkey>,
        rent: &Rent,
        name: &str,
    ) -> Result<(), TransactionBuildError> {
        let add_transaction = !self.inside_transaction();
        if add_transaction {
            self.begin();
        }
        if let Err(err) = self.create_account(
            account.clone(),
            spl_token::state::Mint::LEN,
            &spl_token::id(),
            rent,
            &format!("{} mint", name),
        ) {
            if add_transaction {
                self.rollback();
            }
            return Err(err);
        }

        if let Err(err) = self.add_instruction(
            spl_token::instruction::initialize_mint(
                &spl_token::id(),
                &account.pubkey(),
                mint_authority,
                freeze_authority,
                9,
            )
            .unwrap(),
            format!("Initialize {} mint {}", name, account.pubkey()),
        ) {
            if add_transaction {
                self.rollback();
            }
            return Err(err);
        }

        if add_transaction {
            self.commit();
        }

        Ok(())
    }

    pub fn create_token_account(
        &mut self,
        account: Arc<dyn Signer>,
        mint: &Pubkey,
        authority: &Pubkey,
        rent: &Rent,
        name: &str,
    ) -> Result<(), TransactionBuildError> {
        let add_transaction = !self.inside_transaction();
        if add_transaction {
            self.begin();
        }
        if let Err(err) = self.create_account(
            account.clone(),
            spl_token::state::Account::LEN,
            &spl_token::id(),
            rent,
            &format!("{} token", name),
        ) {
            if add_transaction {
                self.rollback();
            }
            return Err(err);
        }

        if let Err(err) = self.add_instruction(
            spl_token::instruction::initialize_account2(
                &spl_token::id(),
                &account.pubkey(),
                mint,
                authority,
            )
            .unwrap(),
            format!(
                "Initialize {} {} token with mint {}",
                name,
                account.pubkey(),
                mint
            ),
        ) {
            if add_transaction {
                self.rollback();
            }
            return Err(err);
        }

        if add_transaction {
            self.commit();
        }

        Ok(())
    }

    pub fn create_token_account_with_seed(
        &mut self,
        base: Arc<dyn Signer>,
        seed: &str,
        mint: &Pubkey,
        authority: &Pubkey,
        rent: &Rent,
        name: &str,
    ) -> Result<Pubkey, TransactionBuildError> {
        let add_transaction = !self.inside_transaction();
        if add_transaction {
            self.begin();
        }
        let account = match self.create_account_with_seed(
            base.clone(),
            seed,
            spl_token::state::Account::LEN,
            &spl_token::id(),
            rent,
            &format!("{} token", name),
        ) {
            Ok(account) => account,
            Err(err) => {
                if add_transaction {
                    self.rollback();
                }
                return Err(err);
            }
        };
        if let Err(err) = self.add_instruction(
            spl_token::instruction::initialize_account2(
                &spl_token::id(),
                &account,
                mint,
                authority,
            )
            .unwrap(),
            format!(
                "Initialize {} {} token (with seed {} from {}) with mint {}",
                name,
                account,
                seed,
                base.pubkey(),
                mint
            ),
        ) {
            if add_transaction {
                self.rollback();
            }
            return Err(err);
        }

        if add_transaction {
            self.commit();
        }

        Ok(account)
    }

    pub fn create_associated_token_account(
        &mut self,
        wallet_address: &Pubkey,
        mint: &Pubkey,
        name: &str,
    ) -> Result<Pubkey, TransactionBuildError> {
        let account =
            spl_associated_token_account::get_associated_token_address(wallet_address, mint);
        self.add_instruction(
            spl_associated_token_account::create_associated_token_account(
                &self.fee_payer(),
                wallet_address,
                mint,
            ),
            format!(
                "Create {} associated {} mint {} account: {}",
                wallet_address, mint, name, account
            ),
        )?;
        Ok(account)
    }
    /*
    // sends the transaction to check if there are errors before actually executing the transaction
    pub fn get_recent_blockhash(&mut self, client: &RpcClient) -> Result<()> {
        self.recent_blockhash = Some(client.get_recent_blockhash()?.0);
        Ok(())
    }

    // sends the transaction to check if there are errors before actually executing the transaction
    pub fn sign_transaction(&mut self, payer: &Pubkey) -> Result<()> {
        let mut transaction = Transaction::new_with_payer(&self.instructions, Some(&payer));
        transaction.sign(&self.signers, self.recent_blockhash.unwrap());

        self.tx = Some(transaction);
        Ok(())
    }

    // sends the transaction to check if there are errors before actually executing the transaction
    pub fn simulate(&self, client: &RpcClient, show_logs: bool) -> Result<()> {
        let sim_result = client.simulate_transaction(&self.tx.as_ref().unwrap())?;
        //println!("simulate {:?}",sim_result);
        if sim_result.value.err.is_some() || show_logs {
            if let Some(logs) = sim_result.value.logs {
                for log_item in logs.iter() {
                    println!("{:?}", log_item);
                }
            }
        }
        if let Some(err) = sim_result.value.err {
            println!("simulation failed: {:?}", err);
            return Err(anyhow::Error::new(err));
        }
        Ok(())
    }

    pub fn try_send_and_confirm(self, client: &RpcClient) -> Result<Signature> {
        println!("sending...");
        let signature = client.send_and_confirm_transaction(&self.tx.unwrap())?;

        let status = client.get_signature_status(&signature)?.unwrap();

        println!("tx: {} result {:?}", &signature, status);
        status?;

        Ok(signature)
    }*/
}

pub struct Sequence<'a> {
    builder: &'a mut TransactionBuilder,
}

impl<'a> Iterator for Sequence<'a> {
    type Item = PreparedTransaction;

    fn next(&mut self) -> Option<Self::Item> {
        self.builder.build_next()
    }
}

pub struct CombinedSequence<'a> {
    builder: &'a mut TransactionBuilder,
}

impl<'a> Iterator for CombinedSequence<'a> {
    type Item = PreparedTransaction;

    fn next(&mut self) -> Option<Self::Item> {
        self.builder.build_next_combined()
    }
}
