use proc_macro2::{Ident, Span};
use quote::quote;
use syn::*;

pub struct RenameIdent {
    pub source: Ident,
    pub target: Ident,
}

impl Default for RenameIdent {
    fn default() -> Self {
        RenameIdent {
            source: Ident::new("self", Span::call_site()),
            target: parse_quote!(self_),
        }
    }
}

impl RenameIdent {
    pub fn process_block(&self, block: &Block) -> Block {
        Block {
            brace_token: block.brace_token,
            stmts: self.process_stmts(&block.stmts),
        }
    }

    pub fn process_stmts(&self, stmts: impl AsRef<[Stmt]>) -> Vec<Stmt> {
        let stmts = stmts.as_ref();
        let mut result = Vec::with_capacity(stmts.len());

        for stmt in stmts {
            result.push(self.process_stmt(stmt));
        }

        result
    }

    pub fn process_stmt(&self, stmt: &Stmt) -> Stmt {
        match stmt {
            Stmt::Local(local) => Stmt::Local(self.process_local(local)),
            Stmt::Expr(expr, semi) => Stmt::Expr(self.process_expr(expr), semi.clone()),
            other => other.clone(),
        }
    }

    pub fn process_local(&self, local: &Local) -> Local {
        let mut result = local.clone();
        if let Some(LocalInit { expr, .. }) = &mut result.init {
            *expr = Box::new(self.process_expr(&*expr));
        }
        result
    }

    pub fn process_expr(&self, expr: &Expr) -> Expr {
        let mut result = expr.clone();
        match &mut result {
            Expr::Array(ExprArray { elems, .. }) | Expr::Tuple(ExprTuple { elems, .. }) => {
                *elems = elems.iter().map(|el| self.process_expr(el)).collect();
            }
            Expr::Binary(bin) => {
                bin.left = Box::new(self.process_expr(&bin.left));
                bin.right = Box::new(self.process_expr(&bin.right));
            }
            Expr::Block(ExprBlock { block, .. })
            | Expr::Loop(ExprLoop { body: block, .. })
            | Expr::Unsafe(ExprUnsafe { block, .. })
            | Expr::TryBlock(ExprTryBlock { block, .. }) => {
                *block = self.process_block(block);
            }
            Expr::Assign(ExprAssign { right: expr, .. })
            | Expr::Call(ExprCall { func: expr, .. })
            | Expr::Cast(ExprCast { expr, .. })
            | Expr::Group(ExprGroup { expr, .. })
            | Expr::Index(ExprIndex { expr, .. })
            | Expr::Let(ExprLet { expr, .. })
            | Expr::Paren(ExprParen { expr, .. })
            | Expr::Reference(ExprReference { expr, .. })
            | Expr::Field(ExprField { base: expr, .. })
            | Expr::Yield(ExprYield {
                expr: Some(expr), ..
            })
            | Expr::Repeat(ExprRepeat { expr, .. })
            | Expr::Try(ExprTry { expr, .. })
            | Expr::Unary(ExprUnary { expr, .. })
            | Expr::Return(ExprReturn {
                expr: Some(expr), ..
            })
            | Expr::MethodCall(ExprMethodCall { receiver: expr, .. }) => {
                *expr = Box::new(self.process_expr(&expr));
            }
            Expr::ForLoop(ExprForLoop { expr, body, .. }) => {
                *expr = Box::new(self.process_expr(&expr));
                *body = self.process_block(body);
            }
            Expr::If(ExprIf {
                cond,
                then_branch,
                else_branch,
                ..
            }) => {
                *cond = Box::new(self.process_expr(&cond));
                *then_branch = self.process_block(then_branch);
                if let Some((_, else_expr)) = else_branch {
                    *else_expr = Box::new(self.process_expr(expr));
                }
            }
            Expr::Match(ExprMatch { expr, arms, .. }) => {
                *expr = Box::new(self.process_expr(&expr));
                for Arm { guard, body, .. } in arms {
                    if let Some((_, expr)) = guard {
                        *expr = Box::new(self.process_expr(expr));
                    }
                    *body = Box::new(self.process_expr(body));
                }
            }
            Expr::Range(ExprRange { start, end, .. }) => {
                if let Some(start) = start {
                    *start = Box::new(self.process_expr(start));
                }
                if let Some(end) = end {
                    *end = Box::new(self.process_expr(end));
                }
            }
            Expr::Struct(ExprStruct { fields, .. }) => {
                for FieldValue { expr, .. } in fields {
                    *expr = self.process_expr(expr);
                }
            }
            Expr::While(ExprWhile { cond, body, .. }) => {
                *cond = Box::new(self.process_expr(cond));
                *body = self.process_block(body);
            }

            Expr::Path(ExprPath { path, .. }) if path.segments.len() == 1 => {
                let segment = path.segments.first_mut().unwrap();
                if segment.ident == self.source {
                    segment.ident = self.target.clone();
                }
            }

            _ => {}
        }
        result
    }
}

pub struct OCMInfo {
    pub in_name: String,
    pub out_name: String,
    pub signal: bool,
}

pub struct SelfConnectorMapper {
    pub receiver: Ident,
    pub methods: Vec<OCMInfo>,
}

fn expr_is_ident(expr: &Expr, ident: &Ident) -> bool {
    match expr {
        Expr::Path(ExprPath { path, .. }) if path.segments.len() == 1 => {
            let segment = path.segments.first().unwrap();
            segment.ident == *ident
        }
        _ => false,
    }
}

impl SelfConnectorMapper {
    pub fn process_block(&self, block: &Block, ctx_name: &Ident) -> Block {
        Block {
            brace_token: block.brace_token,
            stmts: self.process_stmts(&block.stmts, ctx_name),
        }
    }

    pub fn process_stmts(&self, stmts: impl AsRef<[Stmt]>, ctx_name: &Ident) -> Vec<Stmt> {
        let stmts = stmts.as_ref();
        let mut result = Vec::with_capacity(stmts.len());

        for stmt in stmts {
            result.push(self.process_stmt(stmt, ctx_name));
        }

        result
    }

    pub fn process_stmt(&self, stmt: &Stmt, ctx_name: &Ident) -> Stmt {
        match stmt {
            Stmt::Local(local) => Stmt::Local(self.process_local(local, ctx_name)),
            Stmt::Expr(expr, semi) => Stmt::Expr(self.process_expr(expr, ctx_name), semi.clone()),
            other => other.clone(),
        }
    }

    pub fn process_local(&self, local: &Local, ctx_name: &Ident) -> Local {
        let mut result = local.clone();
        if let Some(LocalInit { expr, .. }) = &mut result.init {
            *expr = Box::new(self.process_expr(expr, ctx_name));
        }
        result
    }

    pub fn process_expr(&self, expr: &Expr, ctx_name: &Ident) -> Expr {
        let mut result = expr.clone();
        match &mut result {
            Expr::Array(ExprArray { elems, .. }) | Expr::Tuple(ExprTuple { elems, .. }) => {
                *elems = elems
                    .iter()
                    .map(|el| self.process_expr(el, ctx_name))
                    .collect();
            }
            Expr::Binary(bin) => {
                bin.left = Box::new(self.process_expr(&bin.left, ctx_name));
                bin.right = Box::new(self.process_expr(&bin.right, ctx_name));
            }
            Expr::Block(ExprBlock { block, .. })
            | Expr::Loop(ExprLoop { body: block, .. })
            | Expr::Unsafe(ExprUnsafe { block, .. })
            | Expr::TryBlock(ExprTryBlock { block, .. }) => {
                *block = self.process_block(block, ctx_name);
            }
            Expr::Assign(ExprAssign { right: expr, .. })
            | Expr::Call(ExprCall { func: expr, .. })
            | Expr::Cast(ExprCast { expr, .. })
            | Expr::Group(ExprGroup { expr, .. })
            | Expr::Index(ExprIndex { expr, .. })
            | Expr::Let(ExprLet { expr, .. })
            | Expr::Paren(ExprParen { expr, .. })
            | Expr::Reference(ExprReference { expr, .. })
            | Expr::Field(ExprField { base: expr, .. })
            | Expr::Yield(ExprYield {
                expr: Some(expr), ..
            })
            | Expr::Repeat(ExprRepeat { expr, .. })
            | Expr::Try(ExprTry { expr, .. })
            | Expr::Unary(ExprUnary { expr, .. })
            | Expr::Return(ExprReturn {
                expr: Some(expr), ..
            }) => {
                *expr = Box::new(self.process_expr(&expr, ctx_name));
            }
            Expr::ForLoop(ExprForLoop { expr, body, .. }) => {
                *expr = Box::new(self.process_expr(&expr, ctx_name));
                *body = self.process_block(body, ctx_name);
            }
            Expr::If(ExprIf {
                cond,
                then_branch,
                else_branch,
                ..
            }) => {
                *cond = Box::new(self.process_expr(&cond, ctx_name));
                *then_branch = self.process_block(then_branch, ctx_name);
                if let Some((_, else_expr)) = else_branch {
                    *else_expr = Box::new(self.process_expr(expr, ctx_name));
                }
            }
            Expr::Match(ExprMatch { expr, arms, .. }) => {
                *expr = Box::new(self.process_expr(&expr, ctx_name));
                for Arm { guard, body, .. } in arms {
                    if let Some((_, expr)) = guard {
                        *expr = Box::new(self.process_expr(expr, ctx_name));
                    }
                    *body = Box::new(self.process_expr(body, ctx_name));
                }
            }
            Expr::Range(ExprRange { start, end, .. }) => {
                if let Some(start) = start {
                    *start = Box::new(self.process_expr(start, ctx_name));
                }
                if let Some(end) = end {
                    *end = Box::new(self.process_expr(end, ctx_name));
                }
            }
            Expr::Struct(ExprStruct { fields, .. }) => {
                for FieldValue { expr, .. } in fields {
                    *expr = self.process_expr(expr, ctx_name);
                }
            }
            Expr::While(ExprWhile { cond, body, .. }) => {
                *cond = Box::new(self.process_expr(cond, ctx_name));
                *body = self.process_block(body, ctx_name);
            }

            Expr::MethodCall(ExprMethodCall {
                receiver,
                method,
                args,
                ..
            }) => {
                if expr_is_ident(&*receiver, &self.receiver) {
                    if let Some(info) = self
                        .methods
                        .iter()
                        .find(|m| m.in_name == method.to_string())
                    {
                        let connector = info.out_name.to_string();
                        let verb = if info.signal {
                            if args.len() > 0 {
                                let args = args.iter();
                                quote! {
                                    ::litesim::prelude::push_event!(#ctx_name, #connector, (), #(#args),*)
                                }
                            } else {
                                quote! {
                                    ::litesim::prelude::push_event!(#ctx_name, #connector, ())
                                }
                            }
                        } else {
                            let args = args.iter();
                            quote! {
                                ::litesim::prelude::push_event!(#ctx_name, #connector, #(#args),*)
                            }
                        };
                        return Expr::Verbatim(verb);
                    }
                }
                *receiver = Box::new(self.process_expr(&receiver, ctx_name));
            }
            _ => {}
        }
        result
    }
}
