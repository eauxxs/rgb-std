// RGB wallet library for smart contracts on Bitcoin & Lightning network
//
// SPDX-License-Identifier: Apache-2.0
//
// Written in 2019-2024 by
//     Dr Maxim Orlovsky <orlovsky@lnp-bp.org>
//
// Copyright (C) 2019-2024 LNP/BP Standards Association. All rights reserved.
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

use std::fmt::{self, Debug, Display, Formatter};
use std::num::ParseIntError;
use std::str::FromStr;

use fluent_uri::enc::EStr;
use fluent_uri::Uri;
use indexmap::IndexMap;
use invoice::{Address, UnknownNetwork};
use percent_encoding::{utf8_percent_encode, AsciiSet, CONTROLS};
use rgb::{ContractId, SecretSeal};
use strict_encoding::{InvalidRString, TypeName};

use crate::invoice::{Beneficiary, ChainNet, InvoiceState, RgbInvoice, RgbTransport, XChainNet};

const OMITTED: &str = "~";
const EXPIRY: &str = "expiry";
const ENDPOINTS: &str = "endpoints";
const TRANSPORT_SEP: char = ',';
const TRANSPORT_HOST_SEP: &str = "://";
const QUERY_ENCODE: &AsciiSet = &CONTROLS
    .add(b' ')
    .add(b'"')
    .add(b'#')
    .add(b'<')
    .add(b'>')
    .add(b'[')
    .add(b']')
    .add(b'&')
    .add(b'=');

#[derive(Clone, PartialEq, Eq, Debug, Display, Error, From)]
#[display(inner)]
pub enum TransportParseError {
    #[display(doc_comments)]
    /// invalid transport {0}.
    InvalidTransport(String),

    #[display(doc_comments)]
    /// invalid transport host {0}.
    InvalidTransportHost(String),
}

#[derive(Clone, PartialEq, Eq, Debug, Display, Error, From)]
#[display(doc_comments)]
pub enum InvoiceParseError {
    #[from]
    #[display(inner)]
    Uri(fluent_uri::ParseError),

    /// invalid invoice.
    Invalid,

    /// contract id is missed from the invoice.
    ContractMissed,

    /// interface information is missed from the invoice.
    IfaceMissed,

    /// assignment data is missed from the invoice.
    AssignmentMissed,

    /// invalid invoice scheme {0}.
    InvalidScheme(String),

    /// no invoice transport has been provided.
    NoTransport,

    /// invalid invoice: contract ID present but no contract interface provided.
    ContractIdNoIface,

    /// invalid contract ID.
    InvalidContractId(String),

    /// invalid interface {0}.
    InvalidIface(String),

    /// invalid expiration timestamp {0}.
    InvalidExpiration(String),

    #[display(inner)]
    #[from]
    InvalidNetwork(UnknownNetwork),

    /// invalid query parameter {0}.
    InvalidQueryParam(String),

    #[from]
    #[display(inner)]
    Id(baid58::Baid58ParseError),

    /// can't recognize beneficiary "{0}": it should be either a bitcoin address
    /// or a blinded UTXO seal.
    Beneficiary(String),

    #[from]
    #[display(inner)]
    Num(ParseIntError),

    /// can't recognize amount "{0}": it should be valid rgb21 allocation
    /// data.
    Data(String),

    #[from]
    /// invalid interface name.
    IfaceName(InvalidRString),
}

impl RgbInvoice {
    fn has_params(&self) -> bool {
        self.expiry.is_some() ||
            self.transports != vec![RgbTransport::UnspecifiedMeans] ||
            !self.unknown_query.is_empty()
    }

    fn query_params(&self) -> IndexMap<String, String> {
        let mut query_params: IndexMap<String, String> = IndexMap::new();
        if let Some(expiry) = self.expiry {
            query_params.insert(EXPIRY.to_string(), expiry.to_string());
        }
        if self.transports != vec![RgbTransport::UnspecifiedMeans] {
            let mut transports: Vec<String> = vec![];
            for transport in self.transports.clone() {
                transports.push(transport.to_string());
            }
            query_params.insert(ENDPOINTS.to_string(), transports.join(&TRANSPORT_SEP.to_string()));
        }
        query_params.extend(self.unknown_query.clone());
        query_params
    }
}

impl Display for RgbTransport {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            RgbTransport::JsonRpc { tls, host } => {
                let s = if *tls { "s" } else { "" };
                write!(f, "rpc{s}{TRANSPORT_HOST_SEP}{}", host)?;
            }
            RgbTransport::RestHttp { tls, host } => {
                let s = if *tls { "s" } else { "" };
                write!(f, "http{s}{TRANSPORT_HOST_SEP}{}", host)?;
            }
            RgbTransport::WebSockets { tls, host } => {
                let s = if *tls { "s" } else { "" };
                write!(f, "ws{s}{TRANSPORT_HOST_SEP}{}", host)?;
            }
            RgbTransport::Storm {} => {
                write!(f, "storm{TRANSPORT_HOST_SEP}_/")?;
            }
            RgbTransport::UnspecifiedMeans => {}
        };
        Ok(())
    }
}

impl FromStr for RgbTransport {
    type Err = TransportParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let tokens = s.split_once(TRANSPORT_HOST_SEP);
        if tokens.is_none() {
            return Err(TransportParseError::InvalidTransport(s.to_string()));
        }
        let (trans_type, host) = tokens.unwrap();
        if host.is_empty() {
            return Err(TransportParseError::InvalidTransportHost(host.to_string()));
        }
        let host = host.to_string();
        let transport = match trans_type {
            "rpc" => RgbTransport::JsonRpc { tls: false, host },
            "rpcs" => RgbTransport::JsonRpc { tls: true, host },
            "http" => RgbTransport::RestHttp { tls: false, host },
            "https" => RgbTransport::RestHttp { tls: true, host },
            "ws" => RgbTransport::WebSockets { tls: false, host },
            "wss" => RgbTransport::WebSockets { tls: true, host },
            "storm" => RgbTransport::Storm {},
            _ => return Err(TransportParseError::InvalidTransport(s.to_string())),
        };
        Ok(transport)
    }
}

impl Display for XChainNet<Beneficiary> {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        write!(f, "{}:", self.chain_network())?;
        match self.into_inner() {
            Beneficiary::BlindedSeal(seal) => Display::fmt(&seal, f),
            Beneficiary::WitnessVout(payload) => {
                let addr = Address::new(payload, self.chain_network().address_network());
                let s = addr.to_string();
                let s = s
                    .trim_start_matches("bc1")
                    .trim_start_matches("tb1")
                    .trim_start_matches("bcrt1");
                // 26 27 34 42 62 -- 14..72
                // TODO: Do address chunking
                f.write_str(s)
            }
        }
    }
}

impl FromStr for ChainNet {
    type Err = InvoiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase() {
            x if ChainNet::BitcoinMainnet.to_string() == x => Ok(ChainNet::BitcoinMainnet),
            x if ChainNet::BitcoinTestnet.to_string() == x => Ok(ChainNet::BitcoinTestnet),
            x if ChainNet::BitcoinSignet.to_string() == x => Ok(ChainNet::BitcoinSignet),
            x if ChainNet::BitcoinRegtest.to_string() == x => Ok(ChainNet::BitcoinRegtest),
            x if ChainNet::LiquidMainnet.to_string() == x => Ok(ChainNet::BitcoinMainnet),
            x if ChainNet::LiquidTestnet.to_string() == x => Ok(ChainNet::LiquidTestnet),
            _ => Err(InvoiceParseError::Beneficiary(s.to_owned())),
        }
    }
}

impl FromStr for XChainNet<Beneficiary> {
    type Err = InvoiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let Some((cn, beneficiary)) = s.split_once(':') else {
            return Err(InvoiceParseError::Beneficiary(s.to_owned()));
        };
        let cn = ChainNet::from_str(cn)?;
        if let Ok(seal) = SecretSeal::from_str(beneficiary) {
            return Ok(XChainNet::with(cn, Beneficiary::BlindedSeal(seal)));
        }

        let prefix = match cn {
            ChainNet::BitcoinMainnet | ChainNet::LiquidMainnet => "bc",
            ChainNet::BitcoinTestnet | ChainNet::BitcoinSignet | ChainNet::LiquidTestnet => "tb",
            ChainNet::BitcoinRegtest => "bcrt",
        };
        let addr = format!("{prefix}1{beneficiary}");
        let payload = Address::from_str(&addr)
            .map_err(|_| InvoiceParseError::Beneficiary(s.to_owned()))?
            .payload;
        Ok(XChainNet::with(cn, Beneficiary::WitnessVout(payload)))
    }
}

impl Display for RgbInvoice {
    fn fmt(&self, f: &mut Formatter) -> fmt::Result {
        let amt = self.owned_state.to_string();
        if let Some(contract) = self.contract {
            Display::fmt(&contract, f)?;
            f.write_str("/")?;
        } else {
            write!(f, "rgb:{OMITTED}/")?;
        }
        if let Some(iface) = self.iface.clone() {
            write!(f, "{iface}/")?;
        } else {
            write!(f, "{OMITTED}/")?;
        }
        if let Some(ref op) = self.operation {
            write!(f, "{op}/")?;
        }
        if let Some(ref assignment_name) = self.assignment {
            write!(f, "{assignment_name}/")?;
        }
        if !amt.is_empty() {
            write!(f, "{amt}+")?;
        }
        Display::fmt(&self.beneficiary, f)?;
        if self.has_params() {
            f.write_str("?")?;
        }
        let query_params = self.query_params();
        for (key, val) in query_params.iter().take(1) {
            write!(
                f,
                "{}={}",
                utf8_percent_encode(key, QUERY_ENCODE),
                utf8_percent_encode(val, QUERY_ENCODE)
            )?;
        }
        for (key, val) in query_params.iter().skip(1) {
            write!(
                f,
                "&{}={}",
                utf8_percent_encode(key, QUERY_ENCODE),
                utf8_percent_encode(val, QUERY_ENCODE)
            )?;
        }
        Ok(())
    }
}

impl FromStr for RgbInvoice {
    type Err = InvoiceParseError;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        let uri = Uri::parse(s)?;

        let scheme = uri.scheme().ok_or(InvoiceParseError::Invalid)?;
        if scheme.as_str() != "rgb" {
            return Err(InvoiceParseError::InvalidScheme(scheme.to_string()));
        }

        let mut path = uri.path().segments();

        let Some(contract_id_str) = path.next() else {
            return Err(InvoiceParseError::ContractMissed);
        };
        let contract = match ContractId::from_str(contract_id_str.as_str()) {
            Ok(cid) => Some(cid),
            Err(_) if contract_id_str.as_str() == OMITTED => None,
            Err(_) => {
                return Err(InvoiceParseError::InvalidContractId(contract_id_str.to_string()));
            }
        };

        let Some(iface_str) = path.next() else {
            return Err(InvoiceParseError::IfaceMissed);
        };
        let iface = match TypeName::try_from(iface_str.to_string()) {
            Ok(i) => Some(i),
            Err(_) if iface_str.as_str() == OMITTED => None,
            Err(_) => return Err(InvoiceParseError::InvalidIface(iface_str.to_string())),
        };
        if contract.is_some() && iface.is_none() {
            return Err(InvoiceParseError::ContractIdNoIface);
        }

        let Some(assignment) = path.next() else {
            return Err(InvoiceParseError::AssignmentMissed);
        };
        let (amount, beneficiary) = assignment
            .as_str()
            .split_once('+')
            .map(|(a, b)| (Some(a), Some(b)))
            .unwrap_or((Some(assignment.as_str()), None));
        // TODO: support other state types
        let (beneficiary_str, value) = match (beneficiary, amount) {
            (Some(b), Some(a)) => (
                b,
                InvoiceState::from_str(a).map_err(|_| InvoiceParseError::Data(a.to_string()))?,
            ),
            (None, Some(b)) => (b, InvoiceState::Void),
            _ => unreachable!(),
        };

        let beneficiary = XChainNet::<Beneficiary>::from_str(beneficiary_str)?;
        let mut query_params = map_query_params(&uri)?;

        let transports = if let Some(endpoints) = query_params.shift_remove(ENDPOINTS) {
            let tokens = endpoints.split(TRANSPORT_SEP);
            let mut transport_vec: Vec<RgbTransport> = vec![];
            for token in tokens {
                transport_vec.push(
                    RgbTransport::from_str(token)
                        .map_err(|e| InvoiceParseError::InvalidQueryParam(e.to_string()))?,
                );
            }
            transport_vec
        } else {
            vec![RgbTransport::UnspecifiedMeans]
        };

        let mut expiry = None;
        if let Some(exp) = query_params.shift_remove(EXPIRY) {
            let timestamp = exp
                .parse::<i64>()
                .map_err(|e| InvoiceParseError::InvalidExpiration(e.to_string()))?;
            expiry = Some(timestamp);
        }

        Ok(RgbInvoice {
            transports,
            contract,
            iface,
            operation: None,
            assignment: None,
            beneficiary,
            owned_state: value,
            expiry,
            unknown_query: query_params,
        })
    }
}

fn percent_decode(estr: &EStr) -> Result<String, InvoiceParseError> {
    Ok(estr
        .decode()
        .into_string()
        .map_err(|e| InvoiceParseError::InvalidQueryParam(e.to_string()))?
        .to_string())
}

fn map_query_params(uri: &Uri<&str>) -> Result<IndexMap<String, String>, InvoiceParseError> {
    let mut map: IndexMap<String, String> = IndexMap::new();
    if let Some(q) = uri.query() {
        let params = q.split('&');
        for p in params {
            if let Some((k, v)) = p.split_once('=') {
                map.insert(percent_decode(k)?, percent_decode(v)?);
            } else {
                return Err(InvoiceParseError::InvalidQueryParam(p.to_string()));
            }
        }
    }
    Ok(map)
}

#[cfg(test)]
mod test {
    use super::*;

    #[test]
    fn parse() {
        // rgb20/rgb25 parameters
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);
        assert_eq!(format!("{invoice:#}"), invoice_str.replace('-', ""));

        // rgb21 parameters
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB21/1@\
                           1+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);
        assert_eq!(format!("{invoice:#}"), invoice_str.replace('-', ""));

        // no amount
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/bc:\
                           utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // no allocation
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB21/bc:\
                           utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // no contract ID
        let invoice_str =
            "rgb:~/RGB20/bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // no contract ID nor iface
        let invoice_str = "rgb:~/~/bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // contract ID provided but no iface
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/~/bc:utxob:\
                           egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::ContractIdNoIface)));

        // invalid contract ID
        let invalid_contract_id = "invalid";
        let invoice_str = format!(
            "rgb:{invalid_contract_id}/RGB20/bc:utxob:\
             egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb"
        );
        let result = RgbInvoice::from_str(&invoice_str);
        assert!(matches!(result,
                Err(InvoiceParseError::InvalidContractId(c)) if c == invalid_contract_id));

        // with expiration
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           expiry=1682086371";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // bad expiration
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           expiry=six";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidExpiration(_))));

        // with bad query parameter
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           expiry";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // with an unknown query parameter
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           unknown=new";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // with two unknown query parameters
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           unknown=new&another=new";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // with expiration and an unknown query parameter
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           expiry=1682086371&unknown=new";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.to_string(), invoice_str);

        // with an unknown query parameter containing percent-encoded text
        let invoice_base = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                            100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?";
        let query_key_encoded = ":@-%20%23";
        let query_key_decoded = ":@- #";
        let query_val_encoded = "?/.%26%3D";
        let query_val_decoded = "?/.&=";
        let invoice =
            RgbInvoice::from_str(&format!("{invoice_base}{query_key_encoded}={query_val_encoded}"))
                .unwrap();
        let query_params = invoice.query_params();
        assert_eq!(query_params[query_key_decoded], query_val_decoded);
        assert_eq!(
            invoice.to_string(),
            format!("{invoice_base}{query_key_encoded}={query_val_encoded}")
        );

        // no scheme
        let invoice_str = "2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/~/bc:utxob:\
                           egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::Invalid)));

        // invalid scheme
        let invoice_str = "bad:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/~/bc:utxob:\
                           egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidScheme(_))));

        // empty transport endpoint specification
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // invalid transport endpoint specification
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=bad";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // invalid transport variant
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpca://host.example.com";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // rgb-rpc variant
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpc://host.example.com";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.transports, vec![RgbTransport::JsonRpc {
            tls: false,
            host: "host.example.com".to_string()
        }]);
        assert_eq!(invoice.to_string(), invoice_str);

        // rgb-rpc variant, host containing authentication, "-" characters and port
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpcs://user:pass@host-1.ex-ample.com:1234";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.transports, vec![RgbTransport::JsonRpc {
            tls: true,
            host: "user:pass@host-1.ex-ample.com:1234".to_string()
        }]);
        assert_eq!(invoice.to_string(), invoice_str);

        // rgb-rpc variant, IPv6 host
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpcs://%5B2001:db8::1%5D:1234";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        assert_eq!(invoice.transports, vec![RgbTransport::JsonRpc {
            tls: true,
            host: "[2001:db8::1]:1234".to_string()
        }]);
        assert_eq!(invoice.to_string(), invoice_str);

        // rgb-rpc variant with missing host
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpc://";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // rgb-rpc variant with invalid separator
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpc/host.example.com";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::InvalidQueryParam(_))));

        // rgb-rpc variant with invalid transport host specification
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=rpc://ho]t";
        let result = RgbInvoice::from_str(invoice_str);
        assert!(matches!(result, Err(InvoiceParseError::Uri(_))));

        // rgb+http variant
        let invoice_str = "rgb:\
                           2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?endpoints=https://\
                           host.example.com";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        let transports = vec![RgbTransport::RestHttp {
            tls: true,
            host: "host.example.com".to_string(),
        }];
        assert_eq!(invoice.transports, transports);
        assert_eq!(invoice.to_string(), invoice_str);

        // rgb+ws variant
        let invoice_str = "rgb:2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?\
                           endpoints=wss://host.example.com";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        let transports = vec![RgbTransport::WebSockets {
            tls: true,
            host: "host.example.com".to_string(),
        }];
        assert_eq!(invoice.transports, transports);
        assert_eq!(invoice.to_string(), invoice_str);

        // TODO: rgb+storm variant

        // multiple transports
        let invoice_str = "rgb:\
                           2WBcas9-yjzEvGufY-9GEgnyMj7-beMNMWA8r-sPHtV1nPU-TMsGMQX/RGB20/\
                           100+bc:utxob:egXsFnw-5Eud7WKYn-7DVQvcPbc-rR69YmgmG-veacwmUFo-uMFKFb?endpoints=rpcs://\
                           host1.example.com,http://host2.example.com,ws://host3.example.com";
        let invoice = RgbInvoice::from_str(invoice_str).unwrap();
        let transports = vec![
            RgbTransport::JsonRpc {
                tls: true,
                host: "host1.example.com".to_string(),
            },
            RgbTransport::RestHttp {
                tls: false,
                host: "host2.example.com".to_string(),
            },
            RgbTransport::WebSockets {
                tls: false,
                host: "host3.example.com".to_string(),
            },
        ];
        assert_eq!(invoice.transports, transports);
        assert_eq!(invoice.to_string(), invoice_str);

        // empty transport parse error
        let result = RgbTransport::from_str("");
        assert!(matches!(result, Err(TransportParseError::InvalidTransport(_))));

        // invalid transport parse error
        let result = RgbTransport::from_str("bad");
        assert!(matches!(result, Err(TransportParseError::InvalidTransport(_))));

        // invalid transport variant parse error
        let result = RgbTransport::from_str("rpca://host.example.com");
        assert!(matches!(result, Err(TransportParseError::InvalidTransport(_))));

        // rgb-rpc variant with missing host parse error
        let result = RgbTransport::from_str("rpc://");
        assert!(matches!(result, Err(TransportParseError::InvalidTransportHost(_))));

        // rgb-rpc variant with invalid separator parse error
        let result = RgbTransport::from_str("rpc/host.example.com");
        assert!(matches!(result, Err(TransportParseError::InvalidTransport(_))));
    }
}
