import { Injectable, signal, computed, inject } from '@angular/core';
import { BehaviorSubject, Observable } from 'rxjs';
import { IngredientIntelligenceService, IngredientBatchStatus, RunCoordinationState } from './ingredient-intelligence.service';

// Workflow state definitions
export type WorkflowState = 
  | 'INITIALIZATION'
  | 'INGREDIENT_SELECTION'
  | 'LOT_VALIDATION'
  | 'BIN_SELECTION'
  | 'QUANTITY_INPUT'
  | 'PICK_CONFIRMATION'
  | 'BATCH_COMPLETION'
  | 'INGREDIENT_SWITCHING'
  | 'AUTO_PROGRESSION'
  | 'RUN_COMPLETION';

export type WorkflowEvent = 
  | 'INITIALIZE_RUN'
  | 'SELECT_INGREDIENT'
  | 'VALIDATE_LOT'
  | 'SELECT_BIN'
  | 'INPUT_QUANTITY'
  | 'CONFIRM_PICK'
  | 'COMPLETE_BATCH'
  | 'TRIGGER_AUTO_SWITCH'
  | 'SWITCH_INGREDIENT'
  | 'COMPLETE_RUN'
  | 'ERROR_OCCURRED'
  | 'RESET_WORKFLOW';

export interface WorkflowContext {
  runNo: number;
  currentIngredient: string;
  selectedLot: string;
  selectedBin: string;
  inputQuantity: number;
  batchNumber: string;
  consecutiveCompletedBatches: number;
  autoSwitchEnabled: boolean;
  switchThreshold: number;
  errorMessage?: string;
  lastAction?: string;
  timestamp: Date;
}

export interface WorkflowTransition {
  from: WorkflowState | '*';
  to: WorkflowState;
  event: WorkflowEvent;
  guard?: (context: WorkflowContext) => boolean;
  action?: (context: WorkflowContext) => void;
}

export interface IngredientSwitchCondition {
  type: 'CONSECUTIVE_BATCHES' | 'TOTAL_BATCHES' | 'USER_INITIATED' | 'ALL_BATCHES_COMPLETE';
  threshold?: number;
  evaluate: (context: WorkflowContext, ingredients: IngredientBatchStatus[]) => boolean;
  description: string;
}

@Injectable({
  providedIn: 'root'
})
export class WorkflowStateMachineService {
  private readonly intelligenceService = inject(IngredientIntelligenceService);

  // Current workflow state
  private currentStateSubject = new BehaviorSubject<WorkflowState>('INITIALIZATION');
  public currentState$ = this.currentStateSubject.asObservable();

  // Workflow context
  private contextSubject = new BehaviorSubject<WorkflowContext>({
    runNo: 0,
    currentIngredient: '',
    selectedLot: '',
    selectedBin: '',
    inputQuantity: 0,
    batchNumber: '',
    consecutiveCompletedBatches: 0,
    autoSwitchEnabled: true,
    switchThreshold: 3,
    timestamp: new Date()
  });
  public context$ = this.contextSubject.asObservable();

  // State signals for reactive UI
  public currentState = signal<WorkflowState>('INITIALIZATION');
  public context = signal<WorkflowContext>(this.contextSubject.value);
  public availableActions = computed(() => this.getAvailableActions(this.currentState()));
  public canAutoSwitch = computed(() => this.canTriggerAutoSwitch(this.context()));
  public nextRecommendedState = computed(() => this.getNextRecommendedState(this.currentState(), this.context()));

  // State transition definitions
  private readonly transitions: WorkflowTransition[] = [
    // Initialization flow
    { from: 'INITIALIZATION', to: 'INGREDIENT_SELECTION', event: 'INITIALIZE_RUN' },
    
    // Ingredient selection flow
    { from: 'INGREDIENT_SELECTION', to: 'LOT_VALIDATION', event: 'SELECT_INGREDIENT' },
    { from: 'INGREDIENT_SELECTION', to: 'AUTO_PROGRESSION', event: 'TRIGGER_AUTO_SWITCH', 
      guard: (ctx) => ctx.autoSwitchEnabled },
    
    // Lot and bin selection flow
    { from: 'LOT_VALIDATION', to: 'BIN_SELECTION', event: 'VALIDATE_LOT' },
    { from: 'BIN_SELECTION', to: 'QUANTITY_INPUT', event: 'SELECT_BIN' },
    
    // Pick execution flow
    { from: 'QUANTITY_INPUT', to: 'PICK_CONFIRMATION', event: 'INPUT_QUANTITY' },
    { from: 'PICK_CONFIRMATION', to: 'BATCH_COMPLETION', event: 'CONFIRM_PICK' },
    
    // Batch completion and switching
    { from: 'BATCH_COMPLETION', to: 'INGREDIENT_SWITCHING', event: 'COMPLETE_BATCH',
      guard: (ctx) => this.shouldTriggerIngredientSwitch(ctx) },
    { from: 'BATCH_COMPLETION', to: 'LOT_VALIDATION', event: 'COMPLETE_BATCH',
      guard: (ctx) => !this.shouldTriggerIngredientSwitch(ctx) },
    
    // Ingredient switching flow
    { from: 'INGREDIENT_SWITCHING', to: 'AUTO_PROGRESSION', event: 'TRIGGER_AUTO_SWITCH' },
    { from: 'AUTO_PROGRESSION', to: 'INGREDIENT_SELECTION', event: 'SWITCH_INGREDIENT' },
    
    // Completion
    { from: 'BATCH_COMPLETION', to: 'RUN_COMPLETION', event: 'COMPLETE_RUN',
      guard: (ctx) => this.isRunComplete(ctx) },
    
    // Error handling and reset
    { from: '*', to: 'INITIALIZATION', event: 'RESET_WORKFLOW' },
    { from: '*', to: 'INITIALIZATION', event: 'ERROR_OCCURRED' }
  ];

  // Ingredient switching conditions
  private readonly switchingConditions: IngredientSwitchCondition[] = [
    {
      type: 'CONSECUTIVE_BATCHES',
      threshold: 3,
      evaluate: (context: WorkflowContext) => context.consecutiveCompletedBatches >= 3,
      description: 'Switch after completing 3 consecutive batches for current ingredient'
    },
    {
      type: 'ALL_BATCHES_COMPLETE',
      evaluate: (context: WorkflowContext, ingredients: IngredientBatchStatus[]) => {
        const currentIngredient = ingredients.find(ing => ing.item_key === context.currentIngredient);
        return currentIngredient?.status === 'AllCompleted' || false;
      },
      description: 'Switch when all batches for current ingredient are completed'
    },
    {
      type: 'USER_INITIATED',
      evaluate: () => false, // Will be set externally
      description: 'Manual user-initiated ingredient switch'
    }
  ];

  /**
   * Initialize workflow for a new run
   */
  initializeWorkflow(runNo: number): void {
    
    const initialContext: WorkflowContext = {
      runNo,
      currentIngredient: '',
      selectedLot: '',
      selectedBin: '',
      inputQuantity: 0,
      batchNumber: '',
      consecutiveCompletedBatches: 0,
      autoSwitchEnabled: true,
      switchThreshold: 3,
      timestamp: new Date()
    };

    this.contextSubject.next(initialContext);
    this.context.set(initialContext);
    this.transitionToState('INITIALIZATION', 'INITIALIZE_RUN');
  }

  /**
   * Transition to a new state with event validation
   */
  transitionToState(newState: WorkflowState, event: WorkflowEvent, contextUpdate?: Partial<WorkflowContext>): boolean {
    const currentState = this.currentStateSubject.value;
    const context = this.contextSubject.value;

    // Find applicable transition
    const transition = this.transitions.find(t => 
      (t.from === currentState || t.from === '*') && t.event === event
    );

    if (!transition) {
      console.warn(`❌ Invalid transition: ${currentState} --${event}--> ${newState}`);
      return false;
    }

    // Check guard condition
    if (transition.guard && !transition.guard(context)) {
      console.warn(`❌ Guard condition failed for transition: ${currentState} --${event}--> ${newState}`);
      return false;
    }

    // Update context if provided
    let updatedContext = context;
    if (contextUpdate) {
      updatedContext = { ...context, ...contextUpdate, timestamp: new Date() };
      this.contextSubject.next(updatedContext);
      this.context.set(updatedContext);
    }

    // Execute transition action
    if (transition.action) {
      transition.action(updatedContext);
    }

    // Update state
    this.currentStateSubject.next(transition.to);
    this.currentState.set(transition.to);

    return true;
  }

  /**
   * Handle ingredient selection
   */
  selectIngredient(itemKey: string): boolean {
    return this.transitionToState('LOT_VALIDATION', 'SELECT_INGREDIENT', {
      currentIngredient: itemKey,
      selectedLot: '',
      selectedBin: '',
      inputQuantity: 0,
      lastAction: `Selected ingredient: ${itemKey}`
    });
  }

  /**
   * Handle lot validation
   */
  validateLot(lotNo: string): boolean {
    return this.transitionToState('BIN_SELECTION', 'VALIDATE_LOT', {
      selectedLot: lotNo,
      selectedBin: '',
      lastAction: `Validated lot: ${lotNo}`
    });
  }

  /**
   * Handle bin selection
   */
  selectBin(binNo: string): boolean {
    return this.transitionToState('QUANTITY_INPUT', 'SELECT_BIN', {
      selectedBin: binNo,
      inputQuantity: 0,
      lastAction: `Selected bin: ${binNo}`
    });
  }

  /**
   * Handle quantity input
   */
  inputQuantity(quantity: number): boolean {
    return this.transitionToState('PICK_CONFIRMATION', 'INPUT_QUANTITY', {
      inputQuantity: quantity,
      lastAction: `Input quantity: ${quantity}`
    });
  }

  /**
   * Handle pick confirmation
   */
  confirmPick(): boolean {
    const context = this.contextSubject.value;
    return this.transitionToState('BATCH_COMPLETION', 'CONFIRM_PICK', {
      lastAction: `Confirmed pick: ${context.inputQuantity} from ${context.selectedLot}/${context.selectedBin}`
    });
  }

  /**
   * Handle batch completion with auto-switching logic
   */
  completeBatch(batchNumber: string): Observable<boolean> {
    const context = this.contextSubject.value;
    const updatedConsecutive = context.consecutiveCompletedBatches + 1;
    
    // Update context with batch completion
    const batchContext: Partial<WorkflowContext> = {
      batchNumber,
      consecutiveCompletedBatches: updatedConsecutive,
      lastAction: `Completed batch: ${batchNumber}`
    };

    // Check if auto-switching should occur
    return new Observable(observer => {
      this.intelligenceService.getCurrentIngredientStatuses();
      const ingredients = this.intelligenceService.getCurrentIngredientStatuses();
      
      const shouldAutoSwitch = this.evaluateSwitchingConditions(
        { ...context, ...batchContext },
        ingredients
      );

      if (shouldAutoSwitch.shouldSwitch) {
        
        // Transition to ingredient switching
        const success = this.transitionToState('INGREDIENT_SWITCHING', 'COMPLETE_BATCH', {
          ...batchContext,
          lastAction: `Auto-switch triggered: ${shouldAutoSwitch.reason}`
        });
        
        observer.next(success);
        observer.complete();
      } else {
        // Continue with same ingredient
        const success = this.transitionToState('LOT_VALIDATION', 'COMPLETE_BATCH', {
          ...batchContext,
          selectedLot: '',
          selectedBin: '',
          inputQuantity: 0
        });
        
        observer.next(success);
        observer.complete();
      }
    });
  }

  /**
   * Handle ingredient switching
   */
  switchToIngredient(newIngredient: string): boolean {
    return this.transitionToState('INGREDIENT_SELECTION', 'SWITCH_INGREDIENT', {
      currentIngredient: newIngredient,
      selectedLot: '',
      selectedBin: '',
      inputQuantity: 0,
      consecutiveCompletedBatches: 0, // Reset counter for new ingredient
      lastAction: `Switched to ingredient: ${newIngredient}`
    });
  }

  /**
   * Evaluate switching conditions
   */
  private evaluateSwitchingConditions(context: WorkflowContext, ingredients: IngredientBatchStatus[]): {
    shouldSwitch: boolean;
    reason: string;
    nextIngredient?: string;
  } {
    // Check each switching condition
    for (const condition of this.switchingConditions) {
      if (condition.evaluate(context, ingredients)) {
        // Find next ingredient
        const nextIngredient = this.findNextIngredient(context.currentIngredient, ingredients);
        
        return {
          shouldSwitch: !!nextIngredient,
          reason: condition.description,
          nextIngredient
        };
      }
    }

    return {
      shouldSwitch: false,
      reason: 'No switching conditions met'
    };
  }

  /**
   * Find next optimal ingredient for switching
   */
  private findNextIngredient(currentIngredient: string, ingredients: IngredientBatchStatus[]): string | undefined {
    // Filter available ingredients (not completed)
    const available = ingredients.filter(ing => 
      ing.status !== 'AllCompleted' && ing.item_key !== currentIngredient
    );

    // Sort by LineId for consistent selection
    available.sort((a, b) => a.line_id - b.line_id);
    
    return available[0]?.item_key;
  }

  /**
   * Check if ingredient switching should be triggered
   */
  private shouldTriggerIngredientSwitch(context: WorkflowContext): boolean {
    return context.autoSwitchEnabled && 
           context.consecutiveCompletedBatches >= context.switchThreshold;
  }

  /**
   * Check if run is complete
   */
  private isRunComplete(context: WorkflowContext): boolean {
    const ingredients = this.intelligenceService.getCurrentIngredientStatuses();
    return ingredients.every(ing => ing.status === 'AllCompleted');
  }

  /**
   * Check if auto-switch can be triggered
   */
  private canTriggerAutoSwitch(context: WorkflowContext): boolean {
    return context.autoSwitchEnabled && 
           context.consecutiveCompletedBatches >= (context.switchThreshold - 1);
  }

  /**
   * Get available actions for current state
   */
  private getAvailableActions(state: WorkflowState): WorkflowEvent[] {
    return this.transitions
      .filter(t => t.from === state || t.from === '*')
      .map(t => t.event);
  }

  /**
   * Get next recommended state based on current state and context
   */
  private getNextRecommendedState(state: WorkflowState, context: WorkflowContext): WorkflowState | null {
    switch (state) {
      case 'INITIALIZATION':
        return 'INGREDIENT_SELECTION';
      case 'INGREDIENT_SELECTION':
        return context.currentIngredient ? 'LOT_VALIDATION' : null;
      case 'LOT_VALIDATION':
        return context.selectedLot ? 'BIN_SELECTION' : null;
      case 'BIN_SELECTION':
        return context.selectedBin ? 'QUANTITY_INPUT' : null;
      case 'QUANTITY_INPUT':
        return context.inputQuantity > 0 ? 'PICK_CONFIRMATION' : null;
      case 'PICK_CONFIRMATION':
        return 'BATCH_COMPLETION';
      case 'BATCH_COMPLETION':
        return this.shouldTriggerIngredientSwitch(context) ? 'INGREDIENT_SWITCHING' : 'LOT_VALIDATION';
      case 'INGREDIENT_SWITCHING':
        return 'AUTO_PROGRESSION';
      case 'AUTO_PROGRESSION':
        return 'INGREDIENT_SELECTION';
      default:
        return null;
    }
  }

  /**
   * Get current workflow status for UI display
   */
  getWorkflowStatus(): {
    state: WorkflowState;
    context: WorkflowContext;
    availableActions: WorkflowEvent[];
    nextRecommendedState: WorkflowState | null;
    canAutoSwitch: boolean;
  } {
    const state = this.currentState();
    const context = this.context();
    
    return {
      state,
      context,
      availableActions: this.availableActions(),
      nextRecommendedState: this.nextRecommendedState(),
      canAutoSwitch: this.canAutoSwitch()
    };
  }

  /**
   * Reset workflow to initial state
   */
  resetWorkflow(): void {
    this.transitionToState('INITIALIZATION', 'RESET_WORKFLOW');
  }

  /**
   * Handle workflow errors
   */
  handleError(errorMessage: string): void {
    console.error(`❌ Workflow error: ${errorMessage}`);
    this.transitionToState('INITIALIZATION', 'ERROR_OCCURRED', {
      errorMessage,
      lastAction: `Error: ${errorMessage}`
    });
  }
}