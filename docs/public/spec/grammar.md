# Grammar

```
Program            → Declaration* EOF

Declaration        → LetDeclaration
                   | MutDeclaration
                   | FunDeclaration
                   | StructDeclaration
                   | EnumDeclaration
                   | ImplBlock
                   | TraitDeclaration
                   | Statement

LetDeclaration     → "let" IDENTIFIER ( ":" Type )? "=" Expression ";"
MutDeclaration     → "mut" IDENTIFIER ( ":" Type )? "=" Expression ";"
FunDeclaration     → "fun" IDENTIFIER GenericParams? "(" Params? ")" ( "->" Type )? Block
StructDeclaration  → "struct" IDENTIFIER GenericParams? "{" StructFields "}"
EnumDeclaration    → "enum" IDENTIFIER GenericParams? "{" EnumVariants "}"
ImplBlock          → "impl" ( Type "for" )? Type "{" FunDeclaration* "}"
TraitDeclaration   → "aspect" IDENTIFIER "{" TraitMethod* "}"
TraitMethod        → "fun" IDENTIFIER "(" Params? ")" ( "->" Type )? ( Block | ";" )

Params             → Param ( "," Param )* ","?
Param              → ( "mut" )? "self" | IDENTIFIER ( ":" Type )?
StructFields       → StructField ( "," StructField )* ","?
StructField        → IDENTIFIER ":" Type
EnumVariants       → EnumVariant ( "," EnumVariant )* ","?
EnumVariant        → IDENTIFIER ( "{" StructFields "}" )?
GenericParams      → "<" GenericParam ( "," GenericParam )* ">"
GenericParam       → IDENTIFIER ( ":" Type )?

Statement          → ExpressionStatement
                   | Block
                   | IfStatement
                   | WhileStatement
                   | ForStatement
                   | LoopStatement
                   | ReturnStatement
                   | BreakStatement
                   | ContinueStatement

ExpressionStatement → Expression ";"
Block               → "{" Declaration* "}"
IfStatement         → "if" "(" Expression ")" Block ( "else" ( IfStatement | Block ) )?
WhileStatement      → "while" "(" Expression ")" Block
ForStatement        → "for" "(" ForInit Expression? ";" Expression? ")" Block
                    | "for" "(" "let" IDENTIFIER "in" Expression ")" Block
ForInit             → MutDeclaration | ExpressionStatement | ";"
LoopStatement       → "loop" Block
ReturnStatement     → "return" Expression? ";"
BreakStatement      → "break" Expression? ";"
ContinueStatement   → "continue" ";"

Expression              → AssignmentExpression
AssignmentExpression    → LValue AssignOp AssignmentExpression | LogicalOrExpression
LValue                  → IDENTIFIER | CallExpression "." IDENTIFIER | CallExpression "[" Expression "]"
AssignOp                → "=" | "+=" | "-=" | "*=" | "/=" | "%="
LogicalOrExpression     → LogicalAndExpression ( "||" LogicalAndExpression )*
LogicalAndExpression    → ComparisonExpression ( "&&" ComparisonExpression )*
ComparisonExpression    → TermExpression ( ( ">" | ">=" | "<" | "<=" | "!=" | "==" ) TermExpression )?
TermExpression          → FactorExpression ( ( "+" | "-" ) FactorExpression )*
FactorExpression        → CastExpression ( ( "*" | "/" | "%" ) CastExpression )*
CastExpression          → AscribeExpression ( "as" Type )*
AscribeExpression       → UnaryExpression ( ":" Type )?
UnaryExpression         → ( "!" | "-" ) UnaryExpression | PostfixExpression
PostfixExpression       → PrimaryExpression ( "(" Arguments? ")" | "." IDENTIFIER | "[" Expression "]" | "?" )*
Arguments               → Expression ( "," Expression )* ","?

PrimaryExpression  → INT | FLOAT | STRING | "true" | "false" | "None" | "()"
                   | "(" Expression ( "," Expression )+ ")"   // tuple
                   | "(" Expression ")"
                   | "[" ( Expression ( "," Expression )* ","? )? "]"  // array literal
                   | IDENTIFIER ( "::" IDENTIFIER )*
                   | StructLiteral
                   | MatchExpression
                   | IfExpression
                   | LoopExpression
                   | ClosureExpression

StructLiteral      → IDENTIFIER ( "::" IDENTIFIER )* "{" FieldInit ( "," FieldInit )* ","? "}"
FieldInit          → IDENTIFIER ( ":" Expression )?   // omitting ": Expression" uses the local variable of the same name

MatchExpression    → "match" Expression "{" MatchArm ( "," MatchArm )* ","? "}"
MatchArm           → Pattern ( "if" Expression )? "=>" Expression
IfExpression       → "if" "(" Expression ")" Block "else" Block
LoopExpression     → "loop" Block
ClosureExpression  → "fun" "(" Params? ")" ( "->" Type )? Block

Pattern            → "_"
                   | "None"
                   | IDENTIFIER
                   | "(" Pattern ( "," Pattern )* ")"          // tuple pattern
                   | IDENTIFIER "::" IDENTIFIER ( "{" PatternFields "}" )?
                   | INT | FLOAT | STRING | "true" | "false"
PatternFields      → IDENTIFIER ( "," IDENTIFIER )*

Type               → IDENTIFIER ( "<" TypeArgs ">" )?
                   | "()"
                   | "(" Type ( "," Type )+ ")"                // tuple type
                   | Type "[]"                                  // array shorthand
                   | "fun" "(" TypeList? ")" ( "->" Type )?    // function type
TypeArgs           → Type ( "," Type )*
TypeList           → Type ( "," Type )*
```
