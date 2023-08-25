use proc_macro2::{Ident, Span};
use syn::{punctuated::Punctuated, spanned::Spanned, *};

use crate::model::ConnectorKind;

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
            Expr::Assign(ExprAssign { left, right, .. }) => {
                *left = Box::new(self.process_expr(left));
                *right = Box::new(self.process_expr(right));
            }
            Expr::Call(ExprCall { func: expr, .. })
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
                    *else_expr = Box::new(self.process_expr(else_expr));
                }
            }
            Expr::Match(ExprMatch { expr, arms, .. }) => {
                *expr = Box::new(self.process_expr(&expr));
                for Arm { guard, body, .. } in arms {
                    if let Some((_, guard_expr)) = guard {
                        *expr = Box::new(self.process_expr(guard_expr));
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
            /*
            // Maybe not the smartest idea. No way of knowing how the underlying macro
            // behaves, so this could cause issues.
            Expr::Macro(ExprMacro { mac, .. }) => {
                mac.tokens = mac
                    .tokens
                    .clone()
                    .into_iter()
                    .map(|token| {
                        if let proc_macro2::TokenTree::Ident(ident) = token {
                            let mapped = if ident == self.source {
                                self.target.clone()
                            } else {
                                ident
                            };
                            TokenTree::Ident(mapped)
                        } else {
                            token
                        }
                    })
                    .collect()
            }
            */
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
    pub kind: ConnectorKind,
    pub in_name: String,
    pub out_name: String,
    pub ty: Type,
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

fn ident_to_expr(ident: Ident) -> Expr {
    let mut segments = Punctuated::new();
    segments.push(PathSegment {
        ident,
        arguments: PathArguments::None,
    });
    Expr::Path(ExprPath {
        attrs: vec![],
        qself: None,
        path: Path {
            leading_colon: None,
            segments,
        },
    })
}

impl SelfConnectorMapper {
    pub fn process_block(&self, block: &Block, ctx_name: &Ident) -> Result<Block> {
        Ok(Block {
            brace_token: block.brace_token,
            stmts: self.process_stmts(&block.stmts, ctx_name)?,
        })
    }

    pub fn process_stmts(&self, stmts: impl AsRef<[Stmt]>, ctx_name: &Ident) -> Result<Vec<Stmt>> {
        let stmts = stmts.as_ref();
        let mut result = Vec::with_capacity(stmts.len());

        for stmt in stmts {
            result.push(self.process_stmt(stmt, ctx_name)?);
        }

        Ok(result)
    }

    pub fn process_stmt(&self, stmt: &Stmt, ctx_name: &Ident) -> Result<Stmt> {
        let result = match stmt {
            Stmt::Local(local) => Stmt::Local(self.process_local(local, ctx_name)?),
            Stmt::Expr(expr, semi) => Stmt::Expr(self.process_expr(expr, ctx_name)?, semi.clone()),
            other => other.clone(),
        };

        Ok(result)
    }

    pub fn process_local(&self, local: &Local, ctx_name: &Ident) -> Result<Local> {
        let mut result = local.clone();
        if let Some(LocalInit { expr, .. }) = &mut result.init {
            *expr = Box::new(self.process_expr(expr, ctx_name)?);
        }

        Ok(result)
    }

    pub fn process_expr(&self, expr: &Expr, ctx_name: &Ident) -> Result<Expr> {
        let mut result = expr.clone();
        match &mut result {
            Expr::Array(ExprArray { elems, .. }) | Expr::Tuple(ExprTuple { elems, .. }) => {
                let mut mapped = Punctuated::new();
                for el in &*elems {
                    mapped.push(self.process_expr(el, ctx_name)?);
                }
                *elems = mapped.into();
            }
            Expr::Binary(bin) => {
                bin.left = Box::new(self.process_expr(&bin.left, ctx_name)?);
                bin.right = Box::new(self.process_expr(&bin.right, ctx_name)?);
            }
            Expr::Block(ExprBlock { block, .. })
            | Expr::Loop(ExprLoop { body: block, .. })
            | Expr::Unsafe(ExprUnsafe { block, .. })
            | Expr::TryBlock(ExprTryBlock { block, .. }) => {
                *block = self.process_block(block, ctx_name)?;
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
                *expr = Box::new(self.process_expr(&expr, ctx_name)?);
            }
            Expr::ForLoop(ExprForLoop { expr, body, .. }) => {
                *expr = Box::new(self.process_expr(&expr, ctx_name)?);
                *body = self.process_block(body, ctx_name)?;
            }
            Expr::If(ExprIf {
                cond,
                then_branch,
                else_branch,
                ..
            }) => {
                *cond = Box::new(self.process_expr(&cond, ctx_name)?);
                *then_branch = self.process_block(then_branch, ctx_name)?;
                if let Some((_, else_expr)) = else_branch {
                    *else_expr = Box::new(self.process_expr(else_expr, ctx_name)?);
                }
            }
            Expr::Match(ExprMatch { expr, arms, .. }) => {
                *expr = Box::new(self.process_expr(&expr, ctx_name)?);
                for Arm { guard, body, .. } in arms {
                    if let Some((_, guard_expr)) = guard {
                        *expr = Box::new(self.process_expr(guard_expr, ctx_name)?);
                    }
                    *body = Box::new(self.process_expr(body, ctx_name)?);
                }
            }
            Expr::Range(ExprRange { start, end, .. }) => {
                if let Some(start) = start {
                    *start = Box::new(self.process_expr(start, ctx_name)?);
                }
                if let Some(end) = end {
                    *end = Box::new(self.process_expr(end, ctx_name)?);
                }
            }
            Expr::Struct(ExprStruct { fields, .. }) => {
                for FieldValue {
                    expr: field_val, ..
                } in fields
                {
                    *field_val = self.process_expr(field_val, ctx_name)?;
                }
            }
            Expr::While(ExprWhile { cond, body, .. }) => {
                *cond = Box::new(self.process_expr(cond, ctx_name)?);
                *body = self.process_block(body, ctx_name)?;
            }

            Expr::MethodCall(call) => {
                let ExprMethodCall {
                    receiver,
                    method,
                    args,

                    attrs,
                    dot_token,
                    turbofish,
                    paren_token,
                } = call.clone();

                if expr_is_ident(&*receiver, &self.receiver) {
                    if let Some(info) = self
                        .methods
                        .iter()
                        .find(|m| m.in_name == method.to_string())
                    {
                        let name = info.out_name.clone();
                        let mut skipped_args = 0;

                        let event: Expr = if info.signal {
                            parse_quote!(::litesim::event::Signal())
                        } else {
                            skipped_args += 1;
                            let msg = args
                                .first()
                                .ok_or_else(|| Error::new(args.span(), "missing event argument"))?;
                            parse_quote!(::litesim::event::Event::new(#msg))
                        };

                        let connector: Expr = parse_quote! {
                            std::borrow::Cow::Borrowed(#name)
                        };

                        let time: Expr = if args.len() >= 1 + skipped_args {
                            args[skipped_args].clone()
                        } else {
                            parse_quote!(::litesim::time::TimeTrigger::Now)
                        };

                        if args.len() >= 2 + skipped_args {
                            let expected = if info.signal {
                                "time can be optionally supplied"
                            } else {
                                "expected event and optionally time"
                            };
                            return Err(Error::new(
                                args[skipped_args + 1].span(),
                                format!("too many arguments; {}", expected),
                            ));
                        }

                        let mut new_args = Punctuated::new();
                        new_args.push(event);
                        new_args.push(connector);
                        new_args.push(time);

                        let method = match info.kind {
                            ConnectorKind::Input => "internal_event_with_time",
                            ConnectorKind::Output => "push_event_with_time",
                        };

                        let mut turbofish_type = Punctuated::new();
                        turbofish_type.push(GenericArgument::Type(info.ty.clone()));
                        let turbofish = Some(AngleBracketedGenericArguments {
                            args: turbofish_type,
                            colon2_token: Some(token::PathSep {
                                spans: [Span::call_site(), Span::call_site()],
                            }),
                            lt_token: token::Lt {
                                spans: [Span::call_site()],
                            },
                            gt_token: token::Gt {
                                spans: [Span::call_site()],
                            },
                        });

                        return Ok(Expr::MethodCall(ExprMethodCall {
                            attrs,
                            receiver: Box::new(ident_to_expr(ctx_name.clone())),
                            dot_token,
                            method: Ident::new(method, Span::call_site()),
                            turbofish,
                            paren_token,
                            args: new_args,
                        }));
                    }
                }

                let new_reciever = self.process_expr(&receiver, ctx_name)?;

                return Ok(Expr::MethodCall(ExprMethodCall {
                    attrs,
                    receiver: Box::new(new_reciever),
                    dot_token,
                    method,
                    turbofish,
                    paren_token,
                    args,
                }));
            }
            _ => {}
        }

        Ok(result)
    }
}
