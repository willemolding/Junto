use hdk::{
    AGENT_ADDRESS,
    error::ZomeApiResult,
    error::ZomeApiError,
    holochain_core_types::{
        cas::content::Address,
        entry::Entry, 
        json::JsonString,
        hash::HashString
    }
};

use std::collections::HashMap;
use multihash::Hash;

//Our modules for holochain actins
use super::definitions::{
    app_definitions,
    function_definitions::{
        FunctionDescriptor,
        FunctionParameters
    }
};

use super::utils;
use super::channel;
use super::user;

//Function to handle the posting of an expression - will link to any specified channels and insert into relevant groups/packs
pub fn handle_post_expression(expression: app_definitions::ExpressionPost, channels: Vec<String>) -> ZomeApiResult<Address>{
    let expression_type = expression.expression_type.clone();
    let mut channels_save = channels.clone();
    let mut query_params: Vec<HashMap<String, String>> = channels.iter().map(|channel| hashmap!{"type".to_string() => "Channel".to_string(), "value".to_string() => channel.to_string()}).collect();
    let mut user_member_packs: Vec<Address> = vec![];

    let entry = Entry::App("expression_post".into(), expression.into());
    let address = hdk::commit_entry(&entry)?;

    match utils::get_links_and_load_type::<String, app_definitions::UserName>(&AGENT_ADDRESS, "username".to_string()){
        Ok(result_vec) => {
            if result_vec.len() > 1{
                return Err(ZomeApiError::from("Post Failed links on user greater than 1".to_string()))
            }
            query_params.push(hashmap!{"type".to_string() => "User".to_string(), "value".to_string() => result_vec[0].entry.username.to_string()});
        },
        Err(hdk_err) => return Err(hdk_err)
    };
    query_params.push(hashmap!{"type".to_string() => "Type".to_string(), "value".to_string() => expression_type.to_string()});
    
    match entry{
        Entry::ChainHeader(header) => {
            let iso_timestamp = serde_json::to_string(header.timestamp());
            match iso_timestamp{
                Ok(iso_timestamp) => {
                    query_params.push(hashmap!{"type".to_string() => "Time:Y".to_string(), "value".to_string() => iso_timestamp[0..4].to_string()}); //add year slice to query params
                    query_params.push(hashmap!{"type".to_string() => "Time:M".to_string(), "value".to_string() => iso_timestamp[5..7].to_string()}); //add month slice to query params
                    query_params.push(hashmap!{"type".to_string() => "Time:D".to_string(), "value".to_string() => iso_timestamp[8..10].to_string()}); //add day slice to query params
                    query_params.push(hashmap!{"type".to_string() => "Time:H".to_string(), "value".to_string() => iso_timestamp[11..13].to_string()}) //add hour slice to query params
                },
                Err(hdk_err) => return Err(ZomeApiError::from(hdk_err.to_string()))
            }
        },
        _ => {}
    }

    query_params.sort_by(|a, b| b["value"].cmp(&a["value"])); //Order vector in reverse alphabetical order
    let user_name_address = user::get_user_username()?.address;

    let den_result = user::get_user_dens(&user_name_address)?;
    let private_den = den_result.private_den;
    let shared_den = den_result.shared_den;
    let public_den = den_result.public_den;

    let user_pack = user::get_user_pack(&user_name_address)?;

    let member_results = user::get_user_member_packs(&user_name_address)?.iter().map(|pack| user_member_packs.push(pack.address.clone()));

    let expression_locals = vec![private_den, shared_den, public_den, user_pack];
    let mut expression_local_hashs = vec![];

    //Refactor for statement to be more rusty
    for expression_local in expression_locals{
        match expression_local{
            Some(value) => {expression_local_hashs.push(value.address.clone())},
            None => return Err(ZomeApiError::from("user is missing a key expression local link".to_string()))
        }
    };

    //Look at using borrows here with lifetime parameters vs clone
    let mut hook_definitions = vec![FunctionDescriptor{name: "global_time_to_expression", parameters: FunctionParameters::GlobalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone()}}, //Link expression to global time objects
                                    FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: HashString::encode_from_str(&hdk::api::DNA_ADDRESS.to_string(), Hash::SHA2256), privacy: app_definitions::Privacy::Public, query_type: "Contextual".to_string(), expression: address.clone()}}, 

                                    FunctionDescriptor{name: "local_time_to_expression", parameters: FunctionParameters::LocalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone(), context: expression_local_hashs[0].clone()}}, //Link expression to private den time objects
                                    FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: expression_local_hashs[0].clone(), privacy: app_definitions::Privacy::Private, query_type: "Standard".to_string(), expression: address.clone()}}, 

                                    FunctionDescriptor{name: "local_time_to_expression", parameters: FunctionParameters::LocalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone(), context: expression_local_hashs[1].clone()}}, //Link expression to shared den time objects
                                    FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: expression_local_hashs[1].clone(), privacy: app_definitions::Privacy::Shared, query_type: "Standard".to_string(), expression: address.clone()}}, 

                                    FunctionDescriptor{name: "local_time_to_expression", parameters: FunctionParameters::LocalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone(), context: expression_local_hashs[2].clone()}}, //Link expression to public den time objects
                                    FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: expression_local_hashs[2].clone(), privacy: app_definitions::Privacy::Public, query_type: "Standard".to_string(), expression: address.clone()}}, 

                                    FunctionDescriptor{name: "local_time_to_expression", parameters: FunctionParameters::LocalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone(), context: expression_local_hashs[3].clone()}}, //Link expression to private den time objects
                                    FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: expression_local_hashs[3].clone(), privacy: app_definitions::Privacy::Shared, query_type: "Standard".to_string(), expression: address.clone()}}];
  
    for pack in user_member_packs{
        hook_definitions.push(FunctionDescriptor{name: "local_time_to_expression", parameters: FunctionParameters::LocalTimeToExpression{tag: "expression", direction: "forward", expression_address: address.clone(), context: pack.clone()}});
        hook_definitions.push(FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_params.clone(), context: pack.clone(), privacy: app_definitions::Privacy::Shared, query_type: "Standard".to_string(), expression: address.clone()}});
    };

    utils::handle_hooks("ExpressionPost".to_string(), hook_definitions)?;
    Ok(address)
}

//Function to handle the resonation of an expression post - will put the post into packs which the post should be resonated into
pub fn handle_resonation(expression: Address, resonation: app_definitions::Resonation) -> ZomeApiResult<String>{
    let expression_post = hdk::utils::get_as_type::<app_definitions::ExpressionPost>(expression.clone())?;
    let user_name_address = user::get_user_username()?.address;
    let user_pack;
    match user::get_user_pack(&user_name_address)?{
        Some(pack) => {user_pack = pack.address;},
        None => return Err(ZomeApiError::from("User has no packs".to_string()))
    };
    let channels = utils::get_links_and_load_type::<String, app_definitions::Channel>(&expression, "channel".to_string())?;
    let times = utils::get_links_and_load_type::<String, app_definitions::Time>(&expression, "time".to_string())?;
    let exp_type = utils::get_links_and_load_type::<String, app_definitions::Channel>(&expression, "type".to_string())?;
    
    let mut query_points: Vec<HashMap<String, String>> = channels.iter().map(|channel| hashmap!{"value".to_string() => channel.entry.name.clone(), "type".to_string() => "Channel".to_string()}).collect();
    times.iter().map(|time| match time.entry.time_type{
                                app_definitions::TimeType::Year => {query_points.push(hashmap!{"value".to_string() => time.entry.time.clone(), "type".to_string() => "Time:Y".to_string()});},
                                app_definitions::TimeType::Month => {query_points.push(hashmap!{"value".to_string() => time.entry.time.clone(), "type".to_string() => "Time:M".to_string()});},
                                app_definitions::TimeType::Day => {query_points.push(hashmap!{"value".to_string() => time.entry.time.clone(), "type".to_string() => "Time:D".to_string()});},
                                app_definitions::TimeType::Hour => {query_points.push(hashmap!{"value".to_string() => time.entry.time.clone(), "type".to_string() => "Time:H".to_string()});}
                            }
                    );
    query_points.push(hashmap!{"value".to_string() => exp_type[0].entry.name.clone(), "type".to_string() => "Type".to_string()});
    
    let mut hook_definitions = vec![FunctionDescriptor{name: "create_query_points", parameters: FunctionParameters::CreateQueryPoints{query_points: query_points.clone(), context: user_pack.clone(), privacy: app_definitions::Privacy::Shared, query_type: "Standard".to_string(), expression: expression.clone()}},
                                    FunctionDescriptor{name: "link_expression", parameters: FunctionParameters::LinkExpression{tag: "resonation", direction: "both", parent_expression: user_pack, child_expression: expression}}];
    utils::handle_hooks("Resonation".to_string(), hook_definitions)?;
    Ok("Resonation generated".to_string())
}

//Function to handle the getting of expression with a given query root and query string
//for example: query_root: Channel: Technology, query_string: Timestamp<2018>:Channel<holochain>:Channel<dht>:User<Eric>
//this would search for all posts in the channel Technology, which where posted in 2018 and also contain the channels Holochain & Dht by the user Eric
//lets see how this function could also be used to get a user for example. all expression = eachother so finding users here should also be possible
//perhaps using query root of application and then using query string Username<User>
// pub fn get_expression(query_root: Address, query_string: String) -> ZomeApiResult<Vec<app_definitions::ExpressionPost>>{
//     json!({"message": "ok"}).into()
// }

pub fn handle_local_query(query_root: Address, query_string: String) -> JsonString{
    json!({"message": "ok"}).into()
}

pub fn handle_global_query(query_root: Address, query_string: String) -> JsonString{
    json!({"message": "ok"}).into()
}