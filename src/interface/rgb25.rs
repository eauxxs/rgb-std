// RGB standard library for working with smart contracts on Bitcoin & Lightning
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2023-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2023-2024 LNP/BP Standards Association. All rights reserved.
//
// Licensed under the Apache License, Version 2.0 (the "License");
// you may not use this file except in compliance with the License.
// You may obtain a copy of the License at
//
//     http://www.apache.org/licenses/LICENSE-2.0
//
// Unless required by applicable law or agreed to in writing, software
// distributed under the License is distributed on an "AS IS" BASIS,
// WITHOUT WARRANTIES OR CONDITIONS OF ANY KIND, either express or implied.
// See the License for the specific language governing permissions and
// limitations under the License.

#![allow(unused_braces)]

use std::fmt::Debug;

use bp::bc::stl::bp_tx_stl;
use invoice::{Amount, Precision};
use rgb::{Occurrences, Types};
use strict_encoding::{StrictDumb, StrictEncode};
use strict_types::stl::std_stl;
use strict_types::{CompileError, LibBuilder, TypeLib};

use super::{
    AssignIface, GenesisIface, GlobalIface, Iface, OwnedIface, Req, TransitionIface, VerNo,
};
use crate::interface::{ContractIface, IfaceId, IfaceWrapper};
use crate::stl::{rgb_contract_stl, AssetTerms, Details, Name, StandardTypes};

pub const LIB_NAME_RGB25: &str = "RGB25";
/// Strict types id for the library providing data types for RGB25 interface.
pub const LIB_ID_RGB25: &str =
    "urn:ubideco:stl:4JmGrg7oTgwuCQtyC4ezC38ToHMzgMCVS5kMSDPwo2ee#camera-betty-bank";

const SUPPLY_MISMATCH: u8 = 1;
const NON_EQUAL_AMOUNTS: u8 = 2;
const INVALID_PROOF: u8 = 3;
const INSUFFICIENT_RESERVES: u8 = 4;
const INSUFFICIENT_COVERAGE: u8 = 5;

#[derive(Copy, Clone, Eq, PartialEq, Hash, Debug)]
#[derive(StrictType, StrictDumb, StrictEncode, StrictDecode)]
#[strict_type(lib = LIB_NAME_RGB25, tags = repr, into_u8, try_from_u8)]
#[repr(u8)]
pub enum Error {
    #[strict_type(dumb)]
    SupplyMismatch = SUPPLY_MISMATCH,
    NonEqualAmounts = NON_EQUAL_AMOUNTS,
    InvalidProof = INVALID_PROOF,
    InsufficientReserves = INSUFFICIENT_RESERVES,
    InsufficientCoverage = INSUFFICIENT_COVERAGE,
}

fn _rgb25_stl() -> Result<TypeLib, CompileError> {
    LibBuilder::new(libname!(LIB_NAME_RGB25), tiny_bset! {
        std_stl().to_dependency(),
        bp_tx_stl().to_dependency(),
        rgb_contract_stl().to_dependency(),
    })
    .transpile::<Error>()
    .compile()
}

/// Generates strict type library providing data types for RGB25 interface.
pub fn rgb25_stl() -> TypeLib { _rgb25_stl().expect("invalid strict type RGB25 library") }

pub fn rgb25() -> Iface {
    let types = StandardTypes::with(rgb25_stl());

    Iface {
        version: VerNo::V1,
        name: tn!("RGB25"),
        global_state: tiny_bmap! {
            fname!("name") => GlobalIface::required(types.get("RGBContract.Name")),
            fname!("details") => GlobalIface::optional(types.get("RGBContract.Details")),
            fname!("precision") => GlobalIface::required(types.get("RGBContract.Precision")),
            fname!("terms") => GlobalIface::required(types.get("RGBContract.AssetTerms")),
            fname!("issuedSupply") => GlobalIface::required(types.get("RGBContract.Amount")),
            fname!("burnedSupply") => GlobalIface::none_or_many(types.get("RGBContract.Amount")),
        },
        assignments: tiny_bmap! {
            fname!("assetOwner") => AssignIface::private(OwnedIface::Amount, Req::OneOrMore),
            fname!("burnRight") => AssignIface::public(OwnedIface::Rights, Req::NoneOrMore),
        },
        valencies: none!(),
        genesis: GenesisIface {
            metadata: Some(types.get("RGBContract.IssueMeta")),
            global: tiny_bmap! {
                fname!("name") => Occurrences::Once,
                fname!("details") => Occurrences::NoneOrOnce,
                fname!("precision") => Occurrences::Once,
                fname!("terms") => Occurrences::Once,
                fname!("issuedSupply") => Occurrences::Once,
            },
            assignments: tiny_bmap! {
                fname!("assetOwner") => Occurrences::OnceOrMore,
            },
            valencies: none!(),
            errors: tiny_bset! {
                SUPPLY_MISMATCH,
                INVALID_PROOF,
                INSUFFICIENT_RESERVES
            },
        },
        transitions: tiny_bmap! {
            tn!("Transfer") => TransitionIface {
                optional: false,
                metadata: None,
                globals: none!(),
                inputs: tiny_bmap! {
                    fname!("assetOwner") => Occurrences::OnceOrMore,
                },
                assignments: tiny_bmap! {
                    fname!("assetOwner") => Occurrences::OnceOrMore,
                },
                valencies: none!(),
                errors: tiny_bset! {
                    NON_EQUAL_AMOUNTS
                },
                default_assignment: Some(fname!("beneficiary")),
            },
            tn!("Burn") => TransitionIface {
                optional: true,
                metadata: Some(types.get("RGBContract.BurnMeta")),
                globals: tiny_bmap! {
                    fname!("burnedSupply") => Occurrences::Once,
                },
                inputs: tiny_bmap! {
                    fname!("burnRight") => Occurrences::Once,
                },
                assignments: tiny_bmap! {
                    fname!("burnRight") => Occurrences::NoneOrOnce,
                },
                valencies: none!(),
                errors: tiny_bset! {
                    SUPPLY_MISMATCH,
                    INVALID_PROOF,
                    INSUFFICIENT_COVERAGE
                },
                default_assignment: None,
            },
        },
        extensions: none!(),
        error_type: types.get("RGB25.Error"),
        default_operation: Some(tn!("Transfer")),
        types: Types::Strict(types.type_system()),
    }
}

#[derive(Wrapper, WrapperMut, Clone, Eq, PartialEq, Debug)]
#[wrapper(Deref)]
#[wrapper_mut(DerefMut)]
pub struct Rgb25(ContractIface);

impl From<ContractIface> for Rgb25 {
    fn from(iface: ContractIface) -> Self {
        if iface.iface.iface_id != Rgb25::IFACE_ID {
            panic!("the provided interface is not RGB25 interface");
        }
        Self(iface)
    }
}

impl IfaceWrapper for Rgb25 {
    const IFACE_NAME: &'static str = LIB_NAME_RGB25;
    const IFACE_ID: IfaceId = IfaceId::from_array([
        0xbb, 0xe4, 0xc0, 0xb9, 0xac, 0xe7, 0x8b, 0x14, 0x92, 0xfc, 0xc5, 0xfa, 0x39, 0x4d, 0x1a,
        0x19, 0x8e, 0x15, 0x42, 0x60, 0xb5, 0x14, 0xb1, 0x33, 0x0c, 0xe9, 0x47, 0x2c, 0x60, 0xdb,
        0x7b, 0x95,
    ]);
}

impl Rgb25 {
    pub fn name(&self) -> Name {
        let strict_val = &self
            .0
            .global("name")
            .expect("RGB25 interface requires global `name`")[0];
        Name::from_strict_val_unchecked(strict_val)
    }

    pub fn details(&self) -> Option<Details> {
        let strict_val = &self
            .0
            .global("details")
            .expect("RGB25 interface requires global `details`");
        if strict_val.len() == 0 {
            None
        } else {
            Some(Details::from_strict_val_unchecked(&strict_val[0]))
        }
    }

    pub fn precision(&self) -> Precision {
        let strict_val = &self
            .0
            .global("precision")
            .expect("RGB25 interface requires global `precision`")[0];
        Precision::from_strict_val_unchecked(strict_val)
    }

    pub fn total_issued_supply(&self) -> Amount {
        self.0
            .global("issuedSupply")
            .expect("RGB25 interface requires global `issuedSupply`")
            .iter()
            .map(Amount::from_strict_val_unchecked)
            .sum()
    }

    pub fn total_burned_supply(&self) -> Amount {
        self.0
            .global("burnedSupply")
            .unwrap_or_default()
            .iter()
            .map(Amount::from_strict_val_unchecked)
            .sum()
    }

    pub fn contract_data(&self) -> AssetTerms {
        let strict_val = &self
            .0
            .global("data")
            .expect("RGB25 interface requires global `data`")[0];
        AssetTerms::from_strict_val_unchecked(strict_val)
    }
}

#[cfg(test)]
mod test {
    use armor::AsciiArmor;

    use super::*;

    const RGB25: &str = include_str!("../../tests/data/rgb25.rgba");

    #[test]
    fn lib_id() {
        let lib = rgb25_stl();
        assert_eq!(lib.id().to_string(), LIB_ID_RGB25);
    }

    #[test]
    fn iface_id() {
        eprintln!("{:#04x?}", rgb25().iface_id().to_byte_array());
        assert_eq!(Rgb25::IFACE_ID, rgb25().iface_id());
    }

    #[test]
    fn iface_creation() { rgb25(); }

    #[test]
    fn iface_bindle() {
        assert_eq!(format!("{}", rgb25().to_ascii_armored_string()), RGB25);
    }
}
